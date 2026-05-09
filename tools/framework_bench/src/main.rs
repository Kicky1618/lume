use std::collections::{BTreeMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

const FRAMEWORKS: &[&str] = &["lume", "react", "vue", "svelte", "solid"];
const RUNTIMES: &[&str] = &["node", "deno", "bun"];
const CASES: &[BenchCase] = &[
    BenchCase {
        name: "counter",
        description: "state updates and event handlers",
        lume_entry: "examples/counter/src/app.lume",
    },
    BenchCase {
        name: "list",
        description: "keyed list rendering and derived filtering",
        lume_entry: "examples/list-picker/src/app.lume",
    },
    BenchCase {
        name: "form",
        description: "controlled inputs and validation-like derived state",
        lume_entry: "examples/form-state/src/app.lume",
    },
    BenchCase {
        name: "input",
        description: "multiple bound inputs and text interpolation",
        lume_entry: "examples/input-preview/src/app.lume",
    },
    BenchCase {
        name: "conditional",
        description: "state-driven conditional branches and toggles",
        lume_entry: "examples/conditional-panel/src/app.lume",
    },
    BenchCase {
        name: "card",
        description: "imported component composition, props, slots, and shared styles",
        lume_entry: "examples/composed-card/src/app.lume",
    },
    BenchCase {
        name: "gallery",
        description: "grid layout with repeated image cards",
        lume_entry: "examples/gallery-grid/src/app.lume",
    },
    BenchCase {
        name: "scoreboard",
        description: "paired counters with conditional status output",
        lume_entry: "examples/scoreboard/src/app.lume",
    },
    BenchCase {
        name: "theme",
        description: "theme tokens, style declarations, and CSS variable output",
        lume_entry: "examples/theme-card/src/app.lume",
    },
    BenchCase {
        name: "routing",
        description: "multi-page route tree and navigation shell",
        lume_entry: "examples/routing-tree/src/app.lume",
    },
    BenchCase {
        name: "workbench",
        description: "larger composed UI with state, loops, conditionals, and actions",
        lume_entry: "examples/mega-workbench/src/app.lume",
    },
];

#[derive(Clone, Copy)]
struct BenchCase {
    name: &'static str,
    description: &'static str,
    lume_entry: &'static str,
}

#[derive(Debug)]
struct Config {
    frameworks: Vec<String>,
    cases: Vec<String>,
    runs: usize,
    warmups: usize,
    runtime: String,
    out: Option<PathBuf>,
    json: bool,
    html: bool,
    keep: bool,
    browser: bool,
    browser_command: Option<String>,
    interactions: usize,
}

#[derive(Debug)]
struct ResultRow {
    framework: String,
    runtime: String,
    case_name: String,
    status: String,
    times: Vec<Duration>,
    load_times: Vec<Duration>,
    interaction_times: Vec<Duration>,
    bundle_bytes: Option<u64>,
    files: Option<usize>,
    dom_nodes: Option<usize>,
    note: Option<String>,
}

#[derive(Debug)]
struct BrowserBench {
    load: Duration,
    interaction: Duration,
    dom_nodes: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::parse(env::args().skip(1).collect())?;
    let workspace = workspace_root()?;
    let work_dir = temp_work_dir();
    fs::create_dir_all(&work_dir)?;

    if config
        .frameworks
        .iter()
        .any(|framework| framework == "lume")
    {
        run_checked(
            Command::new("cargo")
                .arg("build")
                .arg("-q")
                .arg("-p")
                .arg("lume")
                .current_dir(&workspace),
            "build local Lume CLI",
        )?;
    }

    let runtime_available = command_available(runtime_command(&config.runtime));
    let browser_command = if config.browser {
        config
            .browser_command
            .clone()
            .or_else(find_browser_command)
            .ok_or("could not find a browser; pass --browser-command <path>")?
    } else {
        String::new()
    };
    let mut rows = Vec::new();
    for case_name in &config.cases {
        let case = CASES
            .iter()
            .find(|candidate| candidate.name == case_name)
            .ok_or_else(|| format!("unknown case `{case_name}`"))?;
        for framework in &config.frameworks {
            let row = if framework == "lume" {
                bench_lume(&workspace, &work_dir, case, &config, &browser_command)?
            } else if runtime_available {
                bench_js_framework(
                    &workspace,
                    &work_dir,
                    case,
                    framework,
                    &config,
                    &browser_command,
                )?
            } else {
                ResultRow {
                    framework: framework.clone(),
                    runtime: config.runtime.clone(),
                    case_name: case.name.to_string(),
                    status: "skipped".to_string(),
                    times: Vec::new(),
                    load_times: Vec::new(),
                    interaction_times: Vec::new(),
                    bundle_bytes: None,
                    files: None,
                    dom_nodes: None,
                    note: Some(format!(
                        "{} is not installed",
                        runtime_command(&config.runtime)
                    )),
                }
            };
            rows.push(row);
        }
    }

    let report = if config.json {
        render_json(&rows)
    } else if config.html {
        render_html(&rows)
    } else {
        render_markdown(&rows)
    };
    if let Some(out) = &config.out {
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(out, &report)?;
        println!("wrote {}", out.display());
    } else {
        print!("{report}");
    }

    if !config.keep {
        let _ = fs::remove_dir_all(&work_dir);
    } else {
        eprintln!("kept generated projects in {}", work_dir.display());
    }

    Ok(())
}

impl Config {
    fn parse(args: Vec<String>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut config = Config {
            frameworks: FRAMEWORKS.iter().map(|value| value.to_string()).collect(),
            cases: CASES.iter().map(|case| case.name.to_string()).collect(),
            runs: 5,
            warmups: 1,
            runtime: "node".to_string(),
            out: None,
            json: false,
            html: false,
            keep: false,
            browser: false,
            browser_command: None,
            interactions: 20,
        };

        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--framework" | "--frameworks" => {
                    index += 1;
                    config.frameworks = split_values(value_after(&args, index, "--framework")?);
                }
                "--case" | "--cases" => {
                    index += 1;
                    config.cases = split_values(value_after(&args, index, "--case")?);
                }
                "--runs" => {
                    index += 1;
                    config.runs = value_after(&args, index, "--runs")?.parse()?;
                }
                "--warmups" => {
                    index += 1;
                    config.warmups = value_after(&args, index, "--warmups")?.parse()?;
                }
                "--runtime" => {
                    index += 1;
                    config.runtime = value_after(&args, index, "--runtime")?.to_string();
                }
                "--out" => {
                    index += 1;
                    config.out = Some(PathBuf::from(value_after(&args, index, "--out")?));
                }
                "--json" => config.json = true,
                "--html" => config.html = true,
                "--keep" => config.keep = true,
                "--browser" => config.browser = true,
                "--browser-command" => {
                    index += 1;
                    config.browser_command =
                        Some(value_after(&args, index, "--browser-command")?.to_string());
                }
                "--interactions" => {
                    index += 1;
                    config.interactions = value_after(&args, index, "--interactions")?.parse()?;
                }
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown option `{other}`").into()),
            }
            index += 1;
        }

        if config.runs == 0 {
            return Err("--runs must be greater than 0".into());
        }
        if config.interactions == 0 {
            return Err("--interactions must be greater than 0".into());
        }
        if config.json && config.html {
            return Err("--json and --html cannot be used together".into());
        }
        validate_values("framework", &config.frameworks, FRAMEWORKS)?;
        validate_values("runtime", std::slice::from_ref(&config.runtime), RUNTIMES)?;
        validate_values(
            "case",
            &config.cases,
            &CASES.iter().map(|case| case.name).collect::<Vec<_>>(),
        )?;
        Ok(config)
    }
}

fn bench_lume(
    workspace: &Path,
    work_dir: &Path,
    case: &BenchCase,
    config: &Config,
    browser_command: &str,
) -> io::Result<ResultRow> {
    let lume_bin = workspace.join("target/debug/lume");
    let mut times = Vec::new();
    let mut load_times = Vec::new();
    let mut interaction_times = Vec::new();
    let mut bundle_bytes = None;
    let mut files = None;
    let mut dom_nodes = None;
    for run_index in 0..(config.warmups + config.runs) {
        let out_dir = work_dir.join(format!("lume-{}-{run_index}", case.name));
        let started = Instant::now();
        let output = Command::new(&lume_bin)
            .arg("build")
            .arg("--entry")
            .arg(workspace.join(case.lume_entry))
            .arg("--out-dir")
            .arg(&out_dir)
            .current_dir(workspace)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()?;
        if !output.status.success() {
            return Ok(failed_row(
                "lume",
                "lume",
                case.name,
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        let elapsed = started.elapsed();
        if run_index >= config.warmups {
            times.push(elapsed);
            let (bytes, count) = dist_size(&out_dir)?;
            bundle_bytes = Some(bytes);
            files = Some(count);
            if config.browser {
                let browser = bench_browser(browser_command, &out_dir, config.interactions)?;
                load_times.push(browser.load);
                interaction_times.push(browser.interaction);
                dom_nodes = Some(browser.dom_nodes);
            }
        }
    }
    Ok(success_row(
        "lume",
        "lume",
        case.name,
        times,
        load_times,
        interaction_times,
        bundle_bytes,
        files,
        dom_nodes,
    ))
}

fn bench_js_framework(
    workspace: &Path,
    work_dir: &Path,
    case: &BenchCase,
    framework: &str,
    config: &Config,
    browser_command: &str,
) -> io::Result<ResultRow> {
    let project_dir = work_dir.join(format!("{framework}-{}", case.name));
    generate_project(&project_dir, framework, case)?;
    let install = install_command(&config.runtime, &project_dir).output()?;
    if !install.status.success() {
        return Ok(failed_row(
            framework,
            &config.runtime,
            case.name,
            String::from_utf8_lossy(&install.stderr).trim(),
        ));
    }

    let mut times = Vec::new();
    let mut load_times = Vec::new();
    let mut interaction_times = Vec::new();
    let mut bundle_bytes = None;
    let mut files = None;
    let mut dom_nodes = None;
    for run_index in 0..(config.warmups + config.runs) {
        let dist = project_dir.join("dist");
        let _ = fs::remove_dir_all(&dist);
        let started = Instant::now();
        let output = build_command(&config.runtime, &project_dir).output()?;
        if !output.status.success() {
            return Ok(failed_row(
                framework,
                &config.runtime,
                case.name,
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        let elapsed = started.elapsed();
        if run_index >= config.warmups {
            times.push(elapsed);
            let (bytes, count) = dist_size(&dist)?;
            bundle_bytes = Some(bytes);
            files = Some(count);
            if config.browser {
                let browser = bench_browser(browser_command, &dist, config.interactions)?;
                load_times.push(browser.load);
                interaction_times.push(browser.interaction);
                dom_nodes = Some(browser.dom_nodes);
            }
        }
    }

    let _ = workspace;
    Ok(success_row(
        framework,
        &config.runtime,
        case.name,
        times,
        load_times,
        interaction_times,
        bundle_bytes,
        files,
        dom_nodes,
    ))
}

fn generate_project(project_dir: &Path, framework: &str, case: &BenchCase) -> io::Result<()> {
    fs::create_dir_all(project_dir.join("src"))?;
    fs::write(project_dir.join("index.html"), index_html(framework))?;
    fs::write(project_dir.join("package.json"), package_json(framework))?;
    fs::write(project_dir.join("deno.json"), deno_json())?;
    fs::write(project_dir.join("vite.config.js"), vite_config(framework))?;
    fs::write(project_dir.join("src/style.css"), shared_css())?;
    match framework {
        "react" => {
            fs::write(project_dir.join("src/main.jsx"), react_main())?;
            fs::write(project_dir.join("src/App.jsx"), react_app(case.name))?;
        }
        "vue" => {
            fs::write(project_dir.join("src/main.js"), vue_main())?;
            fs::write(project_dir.join("src/App.vue"), vue_app(case.name))?;
        }
        "svelte" => {
            fs::write(project_dir.join("src/main.js"), svelte_main())?;
            fs::write(project_dir.join("src/App.svelte"), svelte_app(case.name))?;
        }
        "solid" => {
            fs::write(project_dir.join("src/main.jsx"), solid_main())?;
            fs::write(project_dir.join("src/App.jsx"), solid_app(case.name))?;
        }
        _ => unreachable!("framework values are validated before project generation"),
    }
    Ok(())
}

fn index_html(framework: &str) -> String {
    let entry = match framework {
        "react" | "solid" => "/src/main.jsx",
        "vue" | "svelte" => "/src/main.js",
        _ => unreachable!(),
    };
    format!(
        r#"<!doctype html>
<html lang="en">
  <head><meta charset="UTF-8" /><meta name="viewport" content="width=device-width, initial-scale=1.0" /><title>Framework bench</title></head>
  <body><div id="root"></div><script type="module" src="{entry}"></script></body>
</html>
"#
    )
}

fn package_json(framework: &str) -> String {
    let deps = match framework {
        "react" => {
            r#""@vitejs/plugin-react":"latest","vite":"latest","react":"latest","react-dom":"latest""#
        }
        "vue" => r#""@vitejs/plugin-vue":"latest","vite":"latest","vue":"latest""#,
        "svelte" => r#""@sveltejs/vite-plugin-svelte":"latest","vite":"latest","svelte":"latest""#,
        "solid" => r#""vite-plugin-solid":"latest","vite":"latest","solid-js":"latest""#,
        _ => unreachable!(),
    };
    format!(
        r#"{{
  "private": true,
  "type": "module",
  "scripts": {{ "build":"vite build --emptyOutDir" }},
  "dependencies": {{ {deps} }},
  "devDependencies": {{}}
}}
"#
    )
}

fn deno_json() -> &'static str {
    r#"{
  "nodeModulesDir": "auto",
  "tasks": {
    "build": "vite build --emptyOutDir"
  }
}
"#
}

fn vite_config(framework: &str) -> &'static str {
    match framework {
        "react" => {
            r#"import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
export default defineConfig({ plugins: [react()] });
"#
        }
        "vue" => {
            r#"import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
export default defineConfig({ plugins: [vue()] });
"#
        }
        "svelte" => {
            r#"import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
export default defineConfig({ plugins: [svelte()] });
"#
        }
        "solid" => {
            r#"import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';
export default defineConfig({ plugins: [solid()] });
"#
        }
        _ => unreachable!(),
    }
}

fn react_main() -> &'static str {
    r#"import React from 'react';
import { createRoot } from 'react-dom/client';
import './style.css';
import App from './App.jsx';
createRoot(document.getElementById('root')).render(<App />);
"#
}

fn react_app(case_name: &str) -> String {
    format!(
        r#"import React, {{ useMemo, useState }} from 'react';

const rows = Array.from({{ length: 240 }}, (_, i) => ({{ id: i + 1, name: `Item ${{i + 1}}`, score: (i * 17) % 101 }}));

export default function App() {{
  const [count, setCount] = useState(0);
  const [query, setQuery] = useState('');
  const [tab, setTab] = useState('overview');
  const [route, setRoute] = useState('dashboard');
  const [form, setForm] = useState({{ name: 'Aki', email: 'aki@example.com', plan: 'Pro' }});
  const filtered = useMemo(() => rows.filter(row => row.name.toLowerCase().includes(query.toLowerCase())), [query]);
  return <main className="shell">
    <h1>{}</h1>
    <nav>{{['dashboard','users','settings'].map(item => <button key={{item}} onClick={{() => setRoute(item)}}>{{item}}</button>)}}</nav>
    <section className="panel"><h2>Counter</h2><button onClick={{() => setCount(count + 1)}}>Increment</button><strong>{{count}}</strong></section>
    <section className="panel"><h2>List</h2><input value={{query}} onChange={{event => setQuery(event.target.value)}} placeholder="Filter" /><ul>{{filtered.slice(0, 80).map(row => <li key={{row.id}}><span>{{row.name}}</span><b>{{row.score}}</b></li>)}}</ul></section>
    <section className="panel"><h2>Form</h2>{{['name','email','plan'].map(field => <label key={{field}}>{{field}}<input value={{form[field]}} onChange={{event => setForm({{...form, [field]: event.target.value}})}} /></label>)}}<p>{{form.email.includes('@') ? 'Ready' : 'Needs email'}}</p></section>
    <section className="panel"><h2>Routing</h2><p>Route: {{route}}</p><p>{{route === 'users' ? `${{filtered.length}} users` : route === 'settings' ? form.plan : 'Dashboard'}}</p></section>
    <section className="panel"><h2>Workbench</h2><div className="tabs">{{['overview','activity','billing'].map(item => <button key={{item}} onClick={{() => setTab(item)}}>{{item}}</button>)}}</div><p>{{tab}} / {{count}} / {{filtered.length}}</p></section>
  </main>;
}}
"#,
        case_name
    )
}

fn vue_main() -> &'static str {
    r#"import { createApp } from 'vue';
import './style.css';
import App from './App.vue';
createApp(App).mount('#root');
"#
}

fn vue_app(case_name: &str) -> String {
    format!(
        r#"<script setup>
import {{ computed, reactive, ref }} from 'vue';
const rows = Array.from({{ length: 240 }}, (_, i) => ({{ id: i + 1, name: `Item ${{i + 1}}`, score: (i * 17) % 101 }}));
const count = ref(0);
const query = ref('');
const tab = ref('overview');
const route = ref('dashboard');
const form = reactive({{ name: 'Aki', email: 'aki@example.com', plan: 'Pro' }});
const filtered = computed(() => rows.filter(row => row.name.toLowerCase().includes(query.value.toLowerCase())));
</script>

<template>
  <main class="shell">
    <h1>{}</h1>
    <nav><button v-for="item in ['dashboard','users','settings']" :key="item" @click="route = item">{{{{ item }}}}</button></nav>
    <section class="panel"><h2>Counter</h2><button @click="count++">Increment</button><strong>{{{{ count }}}}</strong></section>
    <section class="panel"><h2>List</h2><input v-model="query" placeholder="Filter" /><ul><li v-for="row in filtered.slice(0, 80)" :key="row.id"><span>{{{{ row.name }}}}</span><b>{{{{ row.score }}}}</b></li></ul></section>
    <section class="panel"><h2>Form</h2><label>Name<input v-model="form.name" /></label><label>Email<input v-model="form.email" /></label><label>Plan<input v-model="form.plan" /></label><p>{{{{ form.email.includes('@') ? 'Ready' : 'Needs email' }}}}</p></section>
    <section class="panel"><h2>Routing</h2><p>Route: {{{{ route }}}}</p><p>{{{{ route === 'users' ? `${{filtered.length}} users` : route === 'settings' ? form.plan : 'Dashboard' }}}}</p></section>
    <section class="panel"><h2>Workbench</h2><div class="tabs"><button v-for="item in ['overview','activity','billing']" :key="item" @click="tab = item">{{{{ item }}}}</button></div><p>{{{{ tab }}}} / {{{{ count }}}} / {{{{ filtered.length }}}}</p></section>
  </main>
</template>
"#,
        case_name
    )
}

fn svelte_main() -> &'static str {
    r#"import './style.css';
import { mount } from 'svelte';
import App from './App.svelte';
mount(App, { target: document.getElementById('root') });
"#
}

fn svelte_app(case_name: &str) -> String {
    format!(
        r#"<script>
const rows = Array.from({{ length: 240 }}, (_, i) => ({{ id: i + 1, name: `Item ${{i + 1}}`, score: (i * 17) % 101 }}));
let count = 0;
let query = '';
let tab = 'overview';
let route = 'dashboard';
let form = {{ name: 'Aki', email: 'aki@example.com', plan: 'Pro' }};
$: filtered = rows.filter(row => row.name.toLowerCase().includes(query.toLowerCase()));
</script>

<main class="shell">
  <h1>{}</h1>
  <nav>{{#each ['dashboard','users','settings'] as item}}<button on:click={{() => route = item}}>{{item}}</button>{{/each}}</nav>
  <section class="panel"><h2>Counter</h2><button on:click={{() => count += 1}}>Increment</button><strong>{{count}}</strong></section>
  <section class="panel"><h2>List</h2><input bind:value={{query}} placeholder="Filter" /><ul>{{#each filtered.slice(0, 80) as row}}<li><span>{{row.name}}</span><b>{{row.score}}</b></li>{{/each}}</ul></section>
  <section class="panel"><h2>Form</h2><label>Name<input bind:value={{form.name}} /></label><label>Email<input bind:value={{form.email}} /></label><label>Plan<input bind:value={{form.plan}} /></label><p>{{form.email.includes('@') ? 'Ready' : 'Needs email'}}</p></section>
  <section class="panel"><h2>Routing</h2><p>Route: {{route}}</p><p>{{route === 'users' ? `${{filtered.length}} users` : route === 'settings' ? form.plan : 'Dashboard'}}</p></section>
  <section class="panel"><h2>Workbench</h2><div class="tabs">{{#each ['overview','activity','billing'] as item}}<button on:click={{() => tab = item}}>{{item}}</button>{{/each}}</div><p>{{tab}} / {{count}} / {{filtered.length}}</p></section>
</main>
"#,
        case_name
    )
}

fn solid_main() -> &'static str {
    r#"import { render } from 'solid-js/web';
import './style.css';
import App from './App.jsx';
render(() => <App />, document.getElementById('root'));
"#
}

fn solid_app(case_name: &str) -> String {
    format!(
        r#"import {{ createMemo, createSignal, For }} from 'solid-js';

const rows = Array.from({{ length: 240 }}, (_, i) => ({{ id: i + 1, name: `Item ${{i + 1}}`, score: (i * 17) % 101 }}));

export default function App() {{
  const [count, setCount] = createSignal(0);
  const [query, setQuery] = createSignal('');
  const [tab, setTab] = createSignal('overview');
  const [route, setRoute] = createSignal('dashboard');
  const [form, setForm] = createSignal({{ name: 'Aki', email: 'aki@example.com', plan: 'Pro' }});
  const filtered = createMemo(() => rows.filter(row => row.name.toLowerCase().includes(query().toLowerCase())));
  return <main class="shell">
    <h1>{}</h1>
    <nav><For each={{['dashboard','users','settings']}}>{{item => <button onClick={{() => setRoute(item)}}>{{item}}</button>}}</For></nav>
    <section class="panel"><h2>Counter</h2><button onClick={{() => setCount(count() + 1)}}>Increment</button><strong>{{count()}}</strong></section>
    <section class="panel"><h2>List</h2><input value={{query()}} onInput={{event => setQuery(event.currentTarget.value)}} placeholder="Filter" /><ul><For each={{filtered().slice(0, 80)}}>{{row => <li><span>{{row.name}}</span><b>{{row.score}}</b></li>}}</For></ul></section>
    <section class="panel"><h2>Form</h2><For each={{['name','email','plan']}}>{{field => <label>{{field}}<input value={{form()[field]}} onInput={{event => setForm({{...form(), [field]: event.currentTarget.value}})}} /></label>}}</For><p>{{form().email.includes('@') ? 'Ready' : 'Needs email'}}</p></section>
    <section class="panel"><h2>Routing</h2><p>Route: {{route()}}</p><p>{{route() === 'users' ? `${{filtered().length}} users` : route() === 'settings' ? form().plan : 'Dashboard'}}</p></section>
    <section class="panel"><h2>Workbench</h2><div class="tabs"><For each={{['overview','activity','billing']}}>{{item => <button onClick={{() => setTab(item)}}>{{item}}</button>}}</For></div><p>{{tab()}} / {{count()}} / {{filtered().length}}</p></section>
  </main>;
}}
"#,
        case_name
    )
}

fn shared_css() -> &'static str {
    r#"body{margin:0;font-family:system-ui,sans-serif;background:#f7f7f5;color:#202124}.shell{max-width:1040px;margin:0 auto;padding:28px;display:grid;gap:16px}h1{font-size:28px;margin:0}nav,.tabs{display:flex;gap:8px;flex-wrap:wrap}.panel{border:1px solid #ddd;background:#fff;border-radius:8px;padding:16px;display:grid;gap:10px}button,input{font:inherit;border:1px solid #c9c9c4;border-radius:6px;padding:8px 10px;background:white}button{cursor:pointer;background:#202124;color:white}ul{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:8px;list-style:none;padding:0;margin:0}li{display:flex;justify-content:space-between;border:1px solid #eee;padding:8px;border-radius:6px}label{display:grid;gap:4px;max-width:360px}strong{font-size:24px}
"#
}

fn bench_browser(
    browser_command: &str,
    dist: &Path,
    interactions: usize,
) -> io::Result<BrowserBench> {
    let index = dist.join("index.html");
    let original = fs::read_to_string(&index)?;
    let instrumented = inject_browser_bench(&original, interactions);
    fs::write(&index, instrumented)?;
    let output = Command::new(browser_command)
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-sandbox")
        .arg("--allow-file-access-from-files")
        .arg("--run-all-compositor-stages-before-draw")
        .arg("--virtual-time-budget=10000")
        .arg("--dump-dom")
        .arg(format!("file://{}", index.display()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    fs::write(&index, original)?;

    let output = output?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "browser benchmark failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    parse_browser_bench(&String::from_utf8_lossy(&output.stdout))
}

fn inject_browser_bench(html: &str, interactions: usize) -> String {
    let script = format!(
        r#"<script>
(() => {{
  const interactions = {interactions};
  const settle = () => new Promise(resolve => setTimeout(resolve, 0));
  const finish = result => {{
    const pre = document.createElement('pre');
    pre.id = 'lume-browser-bench';
    pre.textContent = '__LUME_BROWSER_BENCH__' + JSON.stringify(result) + '__LUME_BROWSER_BENCH_END__';
    document.body.appendChild(pre);
  }};
  window.addEventListener('load', () => {{
    setTimeout(async () => {{
      try {{
        await settle();
        const navigation = performance.getEntriesByType('navigation')[0];
        const loadMs = navigation && navigation.loadEventEnd > 0 ? navigation.loadEventEnd : performance.now();
        const buttons = Array.from(document.querySelectorAll('button'));
        const inputs = Array.from(document.querySelectorAll('input'));
        const started = performance.now();
        for (let index = 0; index < interactions; index += 1) {{
          for (const input of inputs) {{
            input.value = 'bench-' + index;
            input.dispatchEvent(new Event('input', {{ bubbles: true }}));
            input.dispatchEvent(new Event('change', {{ bubbles: true }}));
          }}
          for (const button of buttons) {{
            button.click();
          }}
          await settle();
        }}
        finish({{
          load_ms: loadMs,
          interaction_ms: performance.now() - started,
          dom_nodes: document.getElementsByTagName('*').length
        }});
      }} catch (error) {{
        finish({{ error: String(error), load_ms: 0, interaction_ms: 0, dom_nodes: 0 }});
      }}
    }}, 0);
  }});
}})();
</script>"#
    );
    if let Some(index) = html.rfind("</body>") {
        let mut out = String::with_capacity(html.len() + script.len());
        out.push_str(&html[..index]);
        out.push_str(&script);
        out.push_str(&html[index..]);
        out
    } else {
        format!("{html}{script}")
    }
}

fn parse_browser_bench(output: &str) -> io::Result<BrowserBench> {
    let start_marker = "__LUME_BROWSER_BENCH__";
    let end_marker = "__LUME_BROWSER_BENCH_END__";
    let start = output.rfind(start_marker).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Other,
            "browser benchmark did not produce a result",
        )
    })? + start_marker.len();
    let end = output[start..]
        .find(end_marker)
        .map(|offset| start + offset)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "browser benchmark result was incomplete",
            )
        })?;
    let json = html_unescape(&output[start..end]);
    let json = json.as_str();
    if let Some(error) = json_string_field(json, "error") {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("browser benchmark script failed: {error}"),
        ));
    }
    let load_ms = json_number_field(json, "load_ms")?;
    let interaction_ms = json_number_field(json, "interaction_ms")?;
    let dom_nodes = json_number_field(json, "dom_nodes")? as usize;
    Ok(BrowserBench {
        load: Duration::from_secs_f64(load_ms / 1000.0),
        interaction: Duration::from_secs_f64(interaction_ms / 1000.0),
        dom_nodes,
    })
}

fn json_number_field(json: &str, field: &str) -> io::Result<f64> {
    let key = format!("\"{field}\":");
    let start = json.find(&key).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("missing browser field `{field}`"),
        )
    })? + key.len();
    let end = json[start..]
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .map(|offset| start + offset)
        .unwrap_or(json.len());
    json[start..end].parse().map_err(|error| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("invalid browser field `{field}`: {error}"),
        )
    })
}

fn json_string_field(json: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\":\"");
    let start = json.find(&key)? + key.len();
    let end = json[start..].find('"')? + start;
    Some(json[start..end].replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn html_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn dist_size(path: &Path) -> io::Result<(u64, usize)> {
    let mut bytes = 0;
    let mut files = 0;
    if !path.exists() {
        return Ok((0, 0));
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            let (child_bytes, child_files) = dist_size(&entry.path())?;
            bytes += child_bytes;
            files += child_files;
        } else {
            bytes += metadata.len();
            files += 1;
        }
    }
    Ok((bytes, files))
}

fn render_markdown(rows: &[ResultRow]) -> String {
    let descriptions = CASES
        .iter()
        .map(|case| (case.name, case.description))
        .collect::<BTreeMap<_, _>>();
    let mut out = String::from("# Framework Benchmarks\n\n");
    out.push_str("| case | framework | runtime | status | build median | build min | build max | load median | interaction median | bundle | files | DOM nodes | note |\n");
    out.push_str(
        "| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n",
    );
    for row in rows {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            row.case_name,
            row.framework,
            row.runtime,
            row.status,
            row.times
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .as_slice()
                .median()
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string()),
            row.times
                .iter()
                .min()
                .copied()
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string()),
            row.times
                .iter()
                .max()
                .copied()
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string()),
            row.load_times
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .as_slice()
                .median()
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string()),
            row.interaction_times
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .as_slice()
                .median()
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string()),
            row.bundle_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "-".to_string()),
            row.files
                .map(|files| files.to_string())
                .unwrap_or_else(|| "-".to_string()),
            row.dom_nodes
                .map(|nodes| nodes.to_string())
                .unwrap_or_else(|| "-".to_string()),
            row.note.as_deref().unwrap_or("")
        ));
    }
    out.push_str("\n## Cases\n\n");
    for case in CASES {
        if descriptions.contains_key(case.name) {
            out.push_str(&format!("- `{}`: {}\n", case.name, case.description));
        }
    }
    out
}

fn render_json(rows: &[ResultRow]) -> String {
    let mut out = String::from("[\n");
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            out.push_str(",\n");
        }
        let times = row
            .times
            .iter()
            .map(|duration| format!("{:.3}", duration.as_secs_f64() * 1000.0))
            .collect::<Vec<_>>()
            .join(",");
        let load_times = row
            .load_times
            .iter()
            .map(|duration| format!("{:.3}", duration.as_secs_f64() * 1000.0))
            .collect::<Vec<_>>()
            .join(",");
        let interaction_times = row
            .interaction_times
            .iter()
            .map(|duration| format!("{:.3}", duration.as_secs_f64() * 1000.0))
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&format!(
            "  {{\"case\":\"{}\",\"framework\":\"{}\",\"runtime\":\"{}\",\"status\":\"{}\",\"times_ms\":[{}],\"median_ms\":{},\"load_times_ms\":[{}],\"load_median_ms\":{},\"interaction_times_ms\":[{}],\"interaction_median_ms\":{},\"bundle_bytes\":{},\"files\":{},\"dom_nodes\":{},\"note\":\"{}\"}}",
            escape_json(&row.case_name),
            escape_json(&row.framework),
            escape_json(&row.runtime),
            escape_json(&row.status),
            times,
            row.times
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .as_slice()
                .median()
                .map(|duration| format!("{:.3}", duration.as_secs_f64() * 1000.0))
                .unwrap_or_else(|| "null".to_string()),
            load_times,
            row.load_times
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .as_slice()
                .median()
                .map(|duration| format!("{:.3}", duration.as_secs_f64() * 1000.0))
                .unwrap_or_else(|| "null".to_string()),
            interaction_times,
            row.interaction_times
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .as_slice()
                .median()
                .map(|duration| format!("{:.3}", duration.as_secs_f64() * 1000.0))
                .unwrap_or_else(|| "null".to_string()),
            row.bundle_bytes.map(|value| value.to_string()).unwrap_or_else(|| "null".to_string()),
            row.files.map(|value| value.to_string()).unwrap_or_else(|| "null".to_string()),
            row.dom_nodes.map(|value| value.to_string()).unwrap_or_else(|| "null".to_string()),
            escape_json(row.note.as_deref().unwrap_or(""))
        ));
    }
    out.push_str("\n]\n");
    out
}

fn render_html(rows: &[ResultRow]) -> String {
    let mut out = String::from(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Framework Benchmarks</title>
  <style>
    :root { color-scheme: light; --bg:#f6f7f8; --panel:#fff; --text:#202124; --muted:#5f6368; --line:#dadce0; --ok:#0b8043; --skip:#8a5a00; --fail:#b3261e; }
    * { box-sizing: border-box; }
    body { margin: 0; font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: var(--bg); color: var(--text); }
    main { max-width: 1180px; margin: 0 auto; padding: 32px 20px 48px; }
    header { display: grid; gap: 8px; margin-bottom: 24px; }
    h1 { margin: 0; font-size: clamp(28px, 4vw, 42px); line-height: 1.05; }
    p { margin: 0; color: var(--muted); }
    table { width: 100%; border-collapse: collapse; background: var(--panel); border: 1px solid var(--line); border-radius: 8px; overflow: hidden; }
    th, td { padding: 10px 12px; border-bottom: 1px solid var(--line); text-align: left; white-space: nowrap; }
    th { background: #eef1f4; font-size: 12px; text-transform: uppercase; letter-spacing: .04em; color: var(--muted); }
    tr:last-child td { border-bottom: 0; }
    td.num, th.num { text-align: right; font-variant-numeric: tabular-nums; }
    .status { display: inline-flex; align-items: center; min-width: 64px; justify-content: center; border-radius: 999px; padding: 3px 8px; font-size: 12px; font-weight: 700; text-transform: uppercase; }
    .status-ok { color: var(--ok); background: #e6f4ea; }
    .status-skipped { color: var(--skip); background: #fef7e0; }
    .status-failed { color: var(--fail); background: #fce8e6; }
    .cases { margin-top: 24px; display: grid; gap: 10px; }
    .case { background: var(--panel); border: 1px solid var(--line); border-radius: 8px; padding: 12px 14px; }
    .case code { font-weight: 700; }
    @media (max-width: 760px) { .table-wrap { overflow-x: auto; } main { padding-inline: 12px; } th, td { padding: 9px 10px; } }
  </style>
</head>
<body>
  <main>
    <header>
      <h1>Framework Benchmarks</h1>
      <p>Build time and emitted bundle size comparison for Lume and generated Vite projects.</p>
    </header>
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Case</th>
            <th>Framework</th>
            <th>Runtime</th>
            <th>Status</th>
            <th class="num">Build Median</th>
            <th class="num">Build Min</th>
            <th class="num">Build Max</th>
            <th class="num">Load Median</th>
            <th class="num">Interaction Median</th>
            <th class="num">Bundle</th>
            <th class="num">Files</th>
            <th class="num">DOM Nodes</th>
            <th>Note</th>
          </tr>
        </thead>
        <tbody>
"#,
    );
    for row in rows {
        let status_class = match row.status.as_str() {
            "ok" => "status-ok",
            "skipped" => "status-skipped",
            "failed" => "status-failed",
            _ => "",
        };
        out.push_str(&format!(
            "          <tr><td>{}</td><td>{}</td><td>{}</td><td><span class=\"status {}\">{}</span></td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td>{}</td></tr>\n",
            escape_html(&row.case_name),
            escape_html(&row.framework),
            escape_html(&row.runtime),
            status_class,
            escape_html(&row.status),
            row.times
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .as_slice()
                .median()
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string()),
            row.times.iter().min().copied().map(format_duration).unwrap_or_else(|| "-".to_string()),
            row.times.iter().max().copied().map(format_duration).unwrap_or_else(|| "-".to_string()),
            row.load_times
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .as_slice()
                .median()
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string()),
            row.interaction_times
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .as_slice()
                .median()
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string()),
            row.bundle_bytes.map(format_bytes).unwrap_or_else(|| "-".to_string()),
            row.files.map(|files| files.to_string()).unwrap_or_else(|| "-".to_string()),
            row.dom_nodes
                .map(|nodes| nodes.to_string())
                .unwrap_or_else(|| "-".to_string()),
            escape_html(row.note.as_deref().unwrap_or(""))
        ));
    }
    out.push_str(
        r#"        </tbody>
      </table>
    </div>
    <section class="cases" aria-label="Benchmark cases">
"#,
    );
    for case in CASES {
        out.push_str(&format!(
            "      <div class=\"case\"><code>{}</code>: {}</div>\n",
            escape_html(case.name),
            escape_html(case.description)
        ));
    }
    out.push_str(
        r#"    </section>
  </main>
</body>
</html>
"#,
    );
    out
}

trait Median {
    fn median(&self) -> Option<Duration>;
}

impl Median for [Duration] {
    fn median(&self) -> Option<Duration> {
        if self.is_empty() {
            return None;
        }
        let mut values = self.to_vec();
        values.sort();
        Some(values[values.len() / 2])
    }
}

fn success_row(
    framework: &str,
    runtime: &str,
    case_name: &str,
    times: Vec<Duration>,
    load_times: Vec<Duration>,
    interaction_times: Vec<Duration>,
    bundle_bytes: Option<u64>,
    files: Option<usize>,
    dom_nodes: Option<usize>,
) -> ResultRow {
    ResultRow {
        framework: framework.to_string(),
        runtime: runtime.to_string(),
        case_name: case_name.to_string(),
        status: "ok".to_string(),
        times,
        load_times,
        interaction_times,
        bundle_bytes,
        files,
        dom_nodes,
        note: None,
    }
}

fn failed_row(framework: &str, runtime: &str, case_name: &str, note: impl ToString) -> ResultRow {
    ResultRow {
        framework: framework.to_string(),
        runtime: runtime.to_string(),
        case_name: case_name.to_string(),
        status: "failed".to_string(),
        times: Vec::new(),
        load_times: Vec::new(),
        interaction_times: Vec::new(),
        bundle_bytes: None,
        files: None,
        dom_nodes: None,
        note: Some(note.to_string().lines().next().unwrap_or("").to_string()),
    }
}

fn workspace_root() -> io::Result<PathBuf> {
    let mut current = env::current_dir()?;
    loop {
        if current.join("Cargo.toml").exists() && current.join("crates/lume_cli").exists() {
            return Ok(current);
        }
        if !current.pop() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "could not find Lume workspace root",
            ));
        }
    }
}

fn temp_work_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    env::temp_dir().join(format!(
        "lume-framework-bench-{}-{stamp}",
        std::process::id()
    ))
}

fn run_checked(command: &mut Command, label: &str) -> io::Result<()> {
    let output = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "{label} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ))
    }
}

fn install_command(runtime: &str, project_dir: &Path) -> Command {
    let mut command = match runtime {
        "node" => {
            let mut command = Command::new("npm");
            command.arg("install").arg("--silent");
            command
        }
        "deno" => {
            let mut command = Command::new("deno");
            command.arg("install");
            command
        }
        "bun" => {
            let mut command = Command::new("bun");
            command.arg("install").arg("--silent");
            command
        }
        _ => unreachable!("runtime values are validated before commands run"),
    };
    command
        .current_dir(project_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

fn build_command(runtime: &str, project_dir: &Path) -> Command {
    let mut command = match runtime {
        "node" => {
            let mut command = Command::new("npm");
            command.arg("run").arg("build").arg("--silent");
            command
        }
        "deno" => {
            let mut command = Command::new("deno");
            command.arg("task").arg("build");
            command
        }
        "bun" => {
            let mut command = Command::new("bun");
            command.arg("run").arg("build");
            command
        }
        _ => unreachable!("runtime values are validated before commands run"),
    };
    command
        .current_dir(project_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

fn runtime_command(runtime: &str) -> &str {
    match runtime {
        "node" => "npm",
        "deno" => "deno",
        "bun" => "bun",
        _ => unreachable!("runtime values are validated before commands run"),
    }
}

fn find_browser_command() -> Option<String> {
    [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ]
    .iter()
    .find(|name| command_available(name))
    .map(|name| name.to_string())
}

fn command_available(name: &str) -> bool {
    let path = match env::var_os("PATH") {
        Some(path) => path,
        None => return false,
    };
    env::split_paths(&path).any(|dir| {
        let candidate = dir.join(name);
        candidate.is_file() || candidate.with_extension(exe_extension()).is_file()
    })
}

fn exe_extension() -> &'static OsStr {
    if cfg!(windows) {
        OsStr::new("exe")
    } else {
        OsStr::new("")
    }
}

fn value_after<'a>(
    args: &'a [String],
    index: usize,
    option: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("missing value for {option}").into())
}

fn split_values(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn validate_values(
    label: &str,
    values: &[String],
    allowed: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let allowed = allowed.iter().copied().collect::<HashSet<_>>();
    for value in values {
        if !allowed.contains(value.as_str()) {
            return Err(format!("unknown {label} `{value}`").into());
        }
    }
    Ok(())
}

fn format_duration(duration: Duration) -> String {
    let millis = duration.as_secs_f64() * 1000.0;
    if millis < 1000.0 {
        format!("{millis:.2}ms")
    } else {
        format!("{:.2}s", millis / 1000.0)
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if (bytes as f64) < MIB {
        format!("{:.1} KiB", bytes as f64 / KIB)
    } else {
        format!("{:.1} MiB", bytes as f64 / MIB)
    }
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn print_help() {
    println!(
        "Compare Lume with generated Vite apps.\n\nUSAGE:\n  cargo run -q -p framework_bench -- [options]\n\nOPTIONS:\n  --framework <list>       Comma-separated: {}\n  --case <list>            Comma-separated: {}\n  --runtime <name>         JS runtime: {}, default node\n  --runs <n>               Measured runs per case, default 5\n  --warmups <n>            Warmup runs per case, default 1\n  --browser                Measure load and interaction speed in headless Chrome\n  --browser-command <path> Browser executable for --browser\n  --interactions <n>       Interaction loops for --browser, default 20\n  --out <path>             Write report to a file\n  --json                   Emit JSON instead of Markdown\n  --html                   Emit HTML instead of Markdown\n  --keep                   Keep generated projects in /tmp\n  -h, --help               Show this help",
        FRAMEWORKS.join(","),
        CASES
            .iter()
            .map(|case| case.name)
            .collect::<Vec<_>>()
            .join(","),
        RUNTIMES.join(",")
    );
}
