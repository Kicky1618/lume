use lume_ffi::{FfiFunction, FfiModule, FfiRegistry, FfiRuntime, FfiType, Repr, SafetyLevel};
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

#[derive(Clone, Debug, Default)]
pub struct NativeBackend;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeArtifact {
    pub target: String,
    pub symbols: Vec<String>,
    pub ffi: NativeBridgePlan,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeBridgePlan {
    pub modules: Vec<NativeBridgeModule>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeBridgeModule {
    pub name: String,
    pub language: String,
    pub library: Option<String>,
    pub header: Option<String>,
    pub sources: Vec<String>,
    pub runtime: Vec<String>,
    pub safety: String,
    pub thread_safe: Option<bool>,
    pub lock: Option<String>,
    pub symbols: Vec<NativeSymbol>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSymbol {
    pub lume_name: String,
    pub native_name: String,
    pub params: Vec<NativeAbiType>,
    pub result: NativeAbiType,
    pub requires_free: Option<String>,
    pub throws: Option<String>,
    pub callback: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeAbiType {
    Void,
    Scalar(String),
    Pointer { mutable: bool, pointee: String },
    Buffer,
    String,
    Handle(String),
    Struct(String),
    Enum { name: String, repr: String },
    Callback(String),
    External(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedNativeModule {
    pub name: String,
    pub library: Option<String>,
    pub symbols: Vec<ResolvedNativeSymbol>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedNativeSymbol {
    pub name: String,
    pub address: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLoadError {
    pub module: String,
    pub symbol: Option<String>,
    pub message: String,
}

pub trait SymbolResolver {
    fn resolve(&self, library: Option<&str>, symbol: &str) -> Result<usize, String>;
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryResolver {
    symbols: HashMap<(Option<String>, String), usize>,
}

#[derive(Debug, Default)]
pub struct DynamicLibraryResolver {
    libraries: std::sync::Mutex<HashMap<Option<String>, NativeLibrary>>,
}

#[derive(Debug)]
struct NativeLibrary {
    handle: *mut c_void,
}

unsafe impl Send for NativeLibrary {}
unsafe impl Sync for NativeLibrary {}

impl NativeBackend {
    pub fn emit_artifact(&self, target: impl Into<String>, symbols: Vec<String>) -> NativeArtifact {
        NativeArtifact {
            target: target.into(),
            symbols,
            ffi: NativeBridgePlan::default(),
        }
    }

    pub fn emit_artifact_with_ffi(
        &self,
        target: impl Into<String>,
        symbols: Vec<String>,
        registry: &FfiRegistry,
    ) -> Result<NativeArtifact, Vec<NativeLoadError>> {
        Ok(NativeArtifact {
            target: target.into(),
            symbols,
            ffi: self.plan_ffi(registry)?,
        })
    }

    pub fn plan_ffi(
        &self,
        registry: &FfiRegistry,
    ) -> Result<NativeBridgePlan, Vec<NativeLoadError>> {
        let report = registry.validate();
        if report.has_errors() {
            return Err(report
                .diagnostics
                .into_iter()
                .filter(|diagnostic| diagnostic.severity == lume_ffi::FfiSeverity::Error)
                .map(|diagnostic| NativeLoadError {
                    module: diagnostic.symbol.unwrap_or_else(|| "ffi".into()),
                    symbol: None,
                    message: diagnostic.message,
                })
                .collect());
        }
        Ok(NativeBridgePlan {
            modules: registry
                .modules
                .iter()
                .map(|module| plan_module(module, &KnownTypes::from_registry(registry)))
                .collect(),
        })
    }
}

impl NativeBridgePlan {
    pub fn resolve<R: SymbolResolver>(
        &self,
        resolver: &R,
    ) -> Result<Vec<ResolvedNativeModule>, Vec<NativeLoadError>> {
        let mut errors = Vec::new();
        let mut modules = Vec::new();
        for module in &self.modules {
            let mut symbols = Vec::new();
            for symbol in &module.symbols {
                match resolver.resolve(module.library.as_deref(), &symbol.native_name) {
                    Ok(address) => symbols.push(ResolvedNativeSymbol {
                        name: symbol.native_name.clone(),
                        address,
                    }),
                    Err(message) => errors.push(NativeLoadError {
                        module: module.name.clone(),
                        symbol: Some(symbol.native_name.clone()),
                        message,
                    }),
                }
            }
            modules.push(ResolvedNativeModule {
                name: module.name.clone(),
                library: module.library.clone(),
                symbols,
            });
        }
        if errors.is_empty() {
            Ok(modules)
        } else {
            Err(errors)
        }
    }

    pub fn to_json(&self) -> String {
        let modules = self
            .modules
            .iter()
            .map(NativeBridgeModule::to_json)
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ \"modules\": [{}] }}", modules)
    }
}

impl NativeBridgeModule {
    fn to_json(&self) -> String {
        let runtime = self
            .runtime
            .iter()
            .map(|value| format!("\"{}\"", escape_json(value)))
            .collect::<Vec<_>>()
            .join(", ");
        let sources = self
            .sources
            .iter()
            .map(|value| format!("\"{}\"", escape_json(value)))
            .collect::<Vec<_>>()
            .join(", ");
        let symbols = self
            .symbols
            .iter()
            .map(NativeSymbol::to_json)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{{ \"name\": \"{}\", \"language\": \"{}\", \"library\": {}, \"header\": {}, \"sources\": [{}], \"runtime\": [{}], \"safety\": \"{}\", \"threadSafe\": {}, \"lock\": {}, \"symbols\": [{}] }}",
            escape_json(&self.name),
            escape_json(&self.language),
            self.library
                .as_ref()
                .map(|value| format!("\"{}\"", escape_json(value)))
                .unwrap_or_else(|| "null".into()),
            self.header
                .as_ref()
                .map(|value| format!("\"{}\"", escape_json(value)))
                .unwrap_or_else(|| "null".into()),
            sources,
            runtime,
            escape_json(&self.safety),
            self.thread_safe
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".into()),
            self.lock
                .as_ref()
                .map(|value| format!("\"{}\"", escape_json(value)))
                .unwrap_or_else(|| "null".into()),
            symbols
        )
    }
}

impl NativeSymbol {
    fn to_json(&self) -> String {
        let params = self
            .params
            .iter()
            .map(NativeAbiType::to_json)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{{ \"lumeName\": \"{}\", \"nativeName\": \"{}\", \"params\": [{}], \"result\": {}, \"requiresFree\": {}, \"throws\": {}, \"callback\": {} }}",
            escape_json(&self.lume_name),
            escape_json(&self.native_name),
            params,
            self.result.to_json(),
            self.requires_free
                .as_ref()
                .map(|value| format!("\"{}\"", escape_json(value)))
                .unwrap_or_else(|| "null".into()),
            self.throws
                .as_ref()
                .map(|value| format!("\"{}\"", escape_json(value)))
                .unwrap_or_else(|| "null".into()),
            self.callback
        )
    }
}

impl NativeAbiType {
    fn to_json(&self) -> String {
        match self {
            Self::Void => "{ \"kind\": \"void\" }".into(),
            Self::Scalar(name) => {
                format!(
                    "{{ \"kind\": \"scalar\", \"name\": \"{}\" }}",
                    escape_json(name)
                )
            }
            Self::Pointer { mutable, pointee } => format!(
                "{{ \"kind\": \"pointer\", \"mutable\": {}, \"pointee\": \"{}\" }}",
                mutable,
                escape_json(pointee)
            ),
            Self::Buffer => "{ \"kind\": \"buffer\" }".into(),
            Self::String => "{ \"kind\": \"string\" }".into(),
            Self::Handle(name) => {
                format!(
                    "{{ \"kind\": \"handle\", \"name\": \"{}\" }}",
                    escape_json(name)
                )
            }
            Self::Struct(name) => {
                format!(
                    "{{ \"kind\": \"struct\", \"name\": \"{}\" }}",
                    escape_json(name)
                )
            }
            Self::Enum { name, repr } => format!(
                "{{ \"kind\": \"enum\", \"name\": \"{}\", \"repr\": \"{}\" }}",
                escape_json(name),
                escape_json(repr)
            ),
            Self::Callback(name) => format!(
                "{{ \"kind\": \"callback\", \"name\": \"{}\" }}",
                escape_json(name)
            ),
            Self::External(name) => format!(
                "{{ \"kind\": \"external\", \"name\": \"{}\" }}",
                escape_json(name)
            ),
        }
    }
}

impl InMemoryResolver {
    pub fn with_symbol(mut self, library: Option<&str>, symbol: &str, address: usize) -> Self {
        self.symbols
            .insert((library.map(str::to_string), symbol.to_string()), address);
        self
    }
}

impl SymbolResolver for InMemoryResolver {
    fn resolve(&self, library: Option<&str>, symbol: &str) -> Result<usize, String> {
        self.symbols
            .get(&(library.map(str::to_string), symbol.to_string()))
            .copied()
            .or_else(|| self.symbols.get(&(None, symbol.to_string())).copied())
            .ok_or_else(|| format!("unresolved native symbol `{symbol}`"))
    }
}

impl SymbolResolver for DynamicLibraryResolver {
    fn resolve(&self, library: Option<&str>, symbol: &str) -> Result<usize, String> {
        let mut libraries = self
            .libraries
            .lock()
            .map_err(|_| "native library resolver lock is poisoned".to_string())?;
        let key = library.map(str::to_string);
        if !libraries.contains_key(&key) {
            let loaded = NativeLibrary::open(library)?;
            libraries.insert(key.clone(), loaded);
        }
        let library = libraries
            .get(&key)
            .ok_or_else(|| "native library was not retained after loading".to_string())?;
        library.symbol(symbol)
    }
}

impl NativeLibrary {
    fn open(path: Option<&str>) -> Result<Self, String> {
        let handle = platform_open(path)?;
        Ok(Self { handle })
    }

    fn symbol(&self, symbol: &str) -> Result<usize, String> {
        let symbol = CString::new(symbol)
            .map_err(|_| "native symbol name contains an interior NUL byte".to_string())?;
        platform_symbol(self.handle, &symbol)
    }
}

impl Drop for NativeLibrary {
    fn drop(&mut self) {
        platform_close(self.handle);
    }
}

#[derive(Clone, Debug, Default)]
struct KnownTypes {
    structs: HashSet<String>,
    enums: HashMap<String, String>,
}

impl KnownTypes {
    fn from_registry(registry: &FfiRegistry) -> Self {
        Self {
            structs: registry
                .structs
                .iter()
                .map(|item| item.name.clone())
                .collect(),
            enums: registry
                .enums
                .iter()
                .map(|item| (item.name.clone(), repr_name(&item.repr)))
                .collect(),
        }
    }
}

fn plan_module(module: &FfiModule, known: &KnownTypes) -> NativeBridgeModule {
    NativeBridgeModule {
        name: module.name.clone(),
        language: module.language.clone(),
        library: module.library.clone(),
        header: module.header.clone(),
        sources: module.sources.clone(),
        runtime: module.runtime.iter().map(runtime_name).collect(),
        safety: match module.safety {
            SafetyLevel::Safe => "safe".into(),
            SafetyLevel::Unsafe => "unsafe".into(),
        },
        thread_safe: module.thread_safe,
        lock: module.lock.clone(),
        symbols: module
            .functions
            .iter()
            .map(|function| plan_symbol(function, known))
            .collect(),
    }
}

fn plan_symbol(function: &FfiFunction, known: &KnownTypes) -> NativeSymbol {
    NativeSymbol {
        lume_name: function.name.clone(),
        native_name: function.name.clone(),
        params: function
            .params
            .iter()
            .map(|param| abi_type(&param.ty, known))
            .collect(),
        result: abi_type(&function.return_ty, known),
        requires_free: function.free.clone(),
        throws: function.throws.clone(),
        callback: function.callback,
    }
}

fn abi_type(ty: &FfiType, known: &KnownTypes) -> NativeAbiType {
    match ty {
        FfiType::Void => NativeAbiType::Void,
        FfiType::Primitive(name) => NativeAbiType::Scalar(name.clone()),
        FfiType::Cstring | FfiType::Utf8String | FfiType::Utf16String => NativeAbiType::String,
        FfiType::Bytes => NativeAbiType::Buffer,
        FfiType::Ptr(inner) => NativeAbiType::Pointer {
            mutable: true,
            pointee: abi_type_name(inner, known),
        },
        FfiType::ConstPtr(inner) => NativeAbiType::Pointer {
            mutable: false,
            pointee: abi_type_name(inner, known),
        },
        FfiType::Borrowed(inner) | FfiType::Owned(inner) | FfiType::View(inner) => {
            abi_type(inner, known)
        }
        FfiType::Handle(name) | FfiType::Opaque(name) => NativeAbiType::Handle(name.clone()),
        FfiType::Struct(name) => NativeAbiType::Struct(name.clone()),
        FfiType::Callback(name) => NativeAbiType::Callback(name.clone()),
        FfiType::Named(name) if known.structs.contains(name) => NativeAbiType::Struct(name.clone()),
        FfiType::Named(name) => known
            .enums
            .get(name)
            .map(|repr| NativeAbiType::Enum {
                name: name.clone(),
                repr: repr.clone(),
            })
            .unwrap_or_else(|| NativeAbiType::External(name.clone())),
    }
}

fn abi_type_name(ty: &FfiType, known: &KnownTypes) -> String {
    match abi_type(ty, known) {
        NativeAbiType::Void => "void".into(),
        NativeAbiType::Scalar(name)
        | NativeAbiType::Handle(name)
        | NativeAbiType::Struct(name)
        | NativeAbiType::Callback(name)
        | NativeAbiType::External(name) => name,
        NativeAbiType::Enum { name, .. } => name,
        NativeAbiType::Pointer { pointee, .. } => format!("ptr<{pointee}>"),
        NativeAbiType::Buffer => "bytes".into(),
        NativeAbiType::String => "string".into(),
    }
}

fn repr_name(repr: &Repr) -> String {
    match repr {
        Repr::C => "C".into(),
        Repr::Packed => "packed".into(),
        Repr::Align(value) => format!("align={value}"),
        Repr::Int(value) | Repr::Custom(value) => value.clone(),
    }
}

fn runtime_name(runtime: &FfiRuntime) -> String {
    match runtime {
        FfiRuntime::Native => "native".into(),
        FfiRuntime::Jit => "jit".into(),
        FfiRuntime::Wasm => "wasm".into(),
        FfiRuntime::Server => "server".into(),
        FfiRuntime::Custom(value) => value.clone(),
    }
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(unix)]
fn platform_open(path: Option<&str>) -> Result<*mut c_void, String> {
    const RTLD_NOW: c_int = 2;
    let path = path
        .map(|value| {
            CString::new(value)
                .map_err(|_| "native library path contains an interior NUL byte".to_string())
        })
        .transpose()?;
    let raw_path = path.as_ref().map_or(std::ptr::null(), |value| value.as_ptr());
    let handle = unsafe { dlopen(raw_path, RTLD_NOW) };
    if handle.is_null() {
        Err(platform_error())
    } else {
        Ok(handle)
    }
}

#[cfg(unix)]
fn platform_symbol(handle: *mut c_void, symbol: &CStr) -> Result<usize, String> {
    let address = unsafe { dlsym(handle, symbol.as_ptr()) };
    if address.is_null() {
        Err(platform_error())
    } else {
        Ok(address as usize)
    }
}

#[cfg(unix)]
fn platform_close(handle: *mut c_void) {
    if !handle.is_null() {
        unsafe {
            dlclose(handle);
        }
    }
}

#[cfg(unix)]
fn platform_error() -> String {
    let message = unsafe { dlerror() };
    if message.is_null() {
        "native dynamic loader error".into()
    } else {
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(unix)]
extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn dlerror() -> *const c_char;
}

#[cfg(windows)]
fn platform_open(path: Option<&str>) -> Result<*mut c_void, String> {
    let path = path.ok_or_else(|| "windows native FFI requires an explicit library".to_string())?;
    let path = CString::new(path)
        .map_err(|_| "native library path contains an interior NUL byte".to_string())?;
    let handle = unsafe { LoadLibraryA(path.as_ptr()) };
    if handle.is_null() {
        Err("failed to load native library".into())
    } else {
        Ok(handle.cast())
    }
}

#[cfg(windows)]
fn platform_symbol(handle: *mut c_void, symbol: &CStr) -> Result<usize, String> {
    let address = unsafe { GetProcAddress(handle.cast(), symbol.as_ptr()) };
    if address.is_null() {
        Err("native symbol was not found".into())
    } else {
        Ok(address as usize)
    }
}

#[cfg(windows)]
fn platform_close(handle: *mut c_void) {
    if !handle.is_null() {
        unsafe {
            FreeLibrary(handle.cast());
        }
    }
}

#[cfg(windows)]
type Hmodule = *mut c_void;

#[cfg(windows)]
extern "system" {
    fn LoadLibraryA(lpLibFileName: *const c_char) -> Hmodule;
    fn GetProcAddress(hModule: Hmodule, lpProcName: *const c_char) -> *mut c_void;
    fn FreeLibrary(hLibModule: Hmodule) -> i32;
}

#[cfg(not(any(unix, windows)))]
fn platform_open(_path: Option<&str>) -> Result<*mut c_void, String> {
    Err("native dynamic loading is not supported on this platform".into())
}

#[cfg(not(any(unix, windows)))]
fn platform_symbol(_handle: *mut c_void, _symbol: &CStr) -> Result<usize, String> {
    Err("native dynamic loading is not supported on this platform".into())
}

#[cfg(not(any(unix, windows)))]
fn platform_close(_handle: *mut c_void) {}

#[cfg(test)]
mod tests {
    use super::*;
    use lume_ffi::{FfiField, Ownership};

    #[test]
    fn skeleton_is_constructible() {
        let _backend = NativeBackend;
    }

    #[test]
    fn emits_native_artifact_metadata() {
        let artifact = NativeBackend.emit_artifact("native", vec!["add".into()]);
        assert_eq!(artifact.target, "native");
        assert_eq!(artifact.symbols, vec!["add"]);
        assert!(artifact.ffi.modules.is_empty());
    }

    #[test]
    fn plans_ffi_bridge_symbols() {
        let mut module = FfiModule::new("geom");
        module.library = Some("./libgeom.so".into());
        module.functions.push(FfiFunction {
            name: "length".into(),
            params: vec![FfiField {
                name: "v".into(),
                ty: FfiType::Named("Vec2".into()),
            }],
            return_ty: FfiType::Primitive("f32".into()),
            ownership: Ownership::Borrowed,
            callback: false,
            free: None,
            throws: None,
        });
        let registry = FfiRegistry {
            modules: vec![module],
            structs: Vec::new(),
            enums: Vec::new(),
            opaques: Vec::new(),
        };
        let plan = NativeBackend.plan_ffi(&registry).expect("plan");
        assert_eq!(plan.modules[0].symbols[0].native_name, "length");
        assert_eq!(
            plan.modules[0].symbols[0].result,
            NativeAbiType::Scalar("f32".into())
        );
    }

    #[test]
    fn resolves_symbols_through_resolver() {
        let plan = NativeBridgePlan {
            modules: vec![NativeBridgeModule {
                name: "geom".into(),
                language: "c".into(),
                library: Some("./libgeom.so".into()),
                header: None,
                sources: Vec::new(),
                runtime: vec!["native".into()],
                safety: "safe".into(),
                thread_safe: None,
                lock: None,
                symbols: vec![NativeSymbol {
                    lume_name: "length".into(),
                    native_name: "length".into(),
                    params: Vec::new(),
                    result: NativeAbiType::Scalar("f32".into()),
                    requires_free: None,
                    throws: None,
                    callback: false,
                }],
            }],
        };
        let resolver =
            InMemoryResolver::default().with_symbol(Some("./libgeom.so"), "length", 0xfeed);
        let resolved = plan.resolve(&resolver).expect("resolved");
        assert_eq!(resolved[0].symbols[0].address, 0xfeed);
    }
}
