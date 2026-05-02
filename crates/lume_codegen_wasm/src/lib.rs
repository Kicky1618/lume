use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple,
};
use inkwell::types::IntType;
use inkwell::values::{FunctionValue, GlobalValue, IntValue};
use inkwell::OptimizationLevel;
use lume_ir::LumeProgram;

const WASM_TARGET: &str = "wasm32-unknown-unknown";

#[derive(Clone, Debug, Default)]
pub struct WasmBackend;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WasmArtifact {
    pub wasm: Vec<u8>,
    pub object: Vec<u8>,
    pub llvm_ir: String,
    pub target: String,
    pub exports: Vec<String>,
}

impl WasmBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn emit(&self, program: &LumeProgram) -> Result<WasmArtifact, String> {
        emit_llvm_wasm(program)
    }

    pub fn emit_skeleton(&self) -> &'static [u8] {
        // A tiny valid module that exports the runtime ABI names expected by
        // the JS loader. This remains as a build-time fallback when the local
        // LLVM install has no WebAssembly target support.
        &[
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
            0x01, 0x0d, 0x02, 0x60, 0x02, 0x7f, 0x7f, 0x00, 0x60, 0x03, 0x7f, 0x7f, 0x7f, 0x01,
            0x7f, // types
            0x03, 0x04, 0x03, 0x00, 0x01, 0x00, // funcs
            0x07, 0x32, 0x03, // exports
            0x09, b'l', b'u', b'm', b'e', b'_', b'i', b'n', b'i', b't', 0x00, 0x00, 0x0d, b'l',
            b'u', b'm', b'e', b'_', b'd', b'i', b's', b'p', b'a', b't', b'c', b'h', 0x00, 0x01,
            0x12, b'l', b'u', b'm', b'e', b'_', b'g', b'e', b't', b'_', b'p', b'a', b't', b'c',
            b'h', b'_', b'l', b'e', b'n', 0x00, 0x02, 0x0a, 0x0e, 0x03, // code
            0x02, 0x00, 0x0b, 0x04, 0x00, 0x41, 0x00, 0x0b, 0x04, 0x00, 0x41, 0x00, 0x0b,
        ]
    }
}

pub fn emit_llvm_wasm(program: &LumeProgram) -> Result<WasmArtifact, String> {
    Target::initialize_webassembly(&InitializationConfig::default());
    let context = Context::create();
    let module = context.create_module("lume_app_wasm");
    let target_machine = wasm_target_machine()?;
    let triple = TargetTriple::create(WASM_TARGET);
    module.set_triple(&triple);
    module.set_data_layout(&target_machine.get_target_data().get_data_layout());

    let mut codegen = WasmCodegen::new(&context, &module);
    codegen.emit(program)?;

    let llvm_ir = module.print_to_string().to_string();
    let object = target_machine
        .write_to_memory_buffer(&module, FileType::Object)
        .map_err(|err| format!("failed to emit wasm32 object from LLVM: {err}"))?
        .as_slice()
        .to_vec();
    let wasm = encode_runtime_wasm(program);

    Ok(WasmArtifact {
        wasm,
        object,
        llvm_ir,
        target: WASM_TARGET.into(),
        exports: vec![
            "lume_init".into(),
            "lume_dispatch".into(),
            "lume_get_patch_len".into(),
            "lume_alloc".into(),
            "lume_free".into(),
        ],
    })
}

fn encode_runtime_wasm(program: &LumeProgram) -> Vec<u8> {
    let mut wasm = Vec::new();
    wasm.extend_from_slice(b"\0asm\x01\0\0\0");
    section(&mut wasm, 1, |out| {
        leb(out, 5);
        func_type(out, &[0x7f, 0x7f], &[]);
        func_type(out, &[0x7f, 0x7f, 0x7f], &[0x7f]);
        func_type(out, &[0x7f], &[0x7f]);
        func_type(out, &[0x7f], &[0x7f]);
        func_type(out, &[0x7f], &[]);
    });
    section(&mut wasm, 3, |out| {
        leb(out, 5);
        for index in 0..5 {
            leb(out, index);
        }
    });
    section(&mut wasm, 5, |out| {
        leb(out, 1);
        out.push(0x00);
        leb(out, 1);
    });
    section(&mut wasm, 6, |out| {
        leb(out, 2 + program.states().count() as u32);
        mutable_i32_global(out, 1024);
        mutable_i32_global(out, 0);
        for state in program.states() {
            mutable_i32_global(out, initial_i32(&state.init.raw) as i32);
        }
    });
    section(&mut wasm, 7, |out| {
        leb(out, 6);
        export(out, "memory", 0x02, 0);
        export(out, "lume_init", 0x00, 0);
        export(out, "lume_dispatch", 0x00, 1);
        export(out, "lume_get_patch_len", 0x00, 2);
        export(out, "lume_alloc", 0x00, 3);
        export(out, "lume_free", 0x00, 4);
    });
    section(&mut wasm, 10, |out| {
        leb(out, 5);
        func_body(out, &[], |body| {
            body.push(0x41);
            leb(body, 0);
            body.push(0x24);
            leb(body, 1);
        });
        func_body(out, &[], |body| {
            body.push(0x41);
            leb(body, 0);
            body.push(0x24);
            leb(body, 1);
            body.push(0x41);
            leb(body, 0);
        });
        func_body(out, &[], |body| {
            body.push(0x23);
            leb(body, 1);
        });
        func_body(out, &[(1, 0x7f)], |body| {
            body.push(0x23);
            leb(body, 0);
            body.push(0x22);
            leb(body, 1);
            body.push(0x23);
            leb(body, 0);
            body.push(0x20);
            leb(body, 0);
            body.push(0x41);
            leb(body, 7);
            body.push(0x6a);
            body.push(0x41);
            sleb(body, -8);
            body.push(0x71);
            body.push(0x6a);
            body.push(0x24);
            leb(body, 0);
            body.push(0x20);
            leb(body, 1);
        });
        func_body(out, &[], |_| {});
    });
    wasm
}

fn section(wasm: &mut Vec<u8>, id: u8, write: impl FnOnce(&mut Vec<u8>)) {
    let mut payload = Vec::new();
    write(&mut payload);
    wasm.push(id);
    leb(wasm, payload.len() as u32);
    wasm.extend(payload);
}

fn func_type(out: &mut Vec<u8>, params: &[u8], results: &[u8]) {
    out.push(0x60);
    leb(out, params.len() as u32);
    out.extend_from_slice(params);
    leb(out, results.len() as u32);
    out.extend_from_slice(results);
}

fn mutable_i32_global(out: &mut Vec<u8>, value: i32) {
    out.push(0x7f);
    out.push(0x01);
    out.push(0x41);
    sleb(out, value);
    out.push(0x0b);
}

fn export(out: &mut Vec<u8>, name: &str, kind: u8, index: u32) {
    leb(out, name.len() as u32);
    out.extend_from_slice(name.as_bytes());
    out.push(kind);
    leb(out, index);
}

fn func_body(out: &mut Vec<u8>, locals: &[(u32, u8)], write: impl FnOnce(&mut Vec<u8>)) {
    let mut body = Vec::new();
    leb(&mut body, locals.len() as u32);
    for (count, ty) in locals {
        leb(&mut body, *count);
        body.push(*ty);
    }
    write(&mut body);
    body.push(0x0b);
    leb(out, body.len() as u32);
    out.extend(body);
}

fn leb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn sleb(out: &mut Vec<u8>, mut value: i32) {
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        out.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}

fn wasm_target_machine() -> Result<TargetMachine, String> {
    let triple = TargetTriple::create(WASM_TARGET);
    let target = Target::from_triple(&triple)
        .map_err(|err| format!("LLVM WebAssembly target `{WASM_TARGET}` is unavailable: {err}"))?;
    target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::Default,
            RelocMode::Static,
            CodeModel::Default,
        )
        .ok_or_else(|| format!("failed to create LLVM target machine for `{WASM_TARGET}`"))
}

struct WasmCodegen<'ctx, 'module> {
    context: &'ctx Context,
    module: &'module Module<'ctx>,
    builder: Builder<'ctx>,
    i32_type: IntType<'ctx>,
    heap_cursor: GlobalValue<'ctx>,
    patch_len: GlobalValue<'ctx>,
}

impl<'ctx, 'module> WasmCodegen<'ctx, 'module> {
    fn new(context: &'ctx Context, module: &'module Module<'ctx>) -> Self {
        let i32_type = context.i32_type();
        let heap_cursor = module.add_global(i32_type, None, "__lume_heap_cursor");
        heap_cursor.set_initializer(&i32_type.const_int(1024, false));
        heap_cursor.set_linkage(Linkage::Internal);

        let patch_len = module.add_global(i32_type, None, "__lume_patch_len");
        patch_len.set_initializer(&i32_type.const_zero());
        patch_len.set_linkage(Linkage::Internal);

        Self {
            context,
            module,
            builder: context.create_builder(),
            i32_type,
            heap_cursor,
            patch_len,
        }
    }

    fn emit(&mut self, program: &LumeProgram) -> Result<(), String> {
        self.emit_state_globals(program);
        self.emit_init()?;
        self.emit_dispatch()?;
        self.emit_get_patch_len()?;
        self.emit_alloc()?;
        self.emit_free()?;
        Ok(())
    }

    fn emit_state_globals(&self, program: &LumeProgram) {
        for (index, state) in program.states().enumerate() {
            let global = self.module.add_global(
                self.i32_type,
                None,
                &format!("__lume_state_{index}_{}", state.name),
            );
            global.set_initializer(&self.i32_type.const_int(initial_i32(&state.init.raw), true));
            global.set_linkage(Linkage::Internal);
        }
    }

    fn emit_init(&self) -> Result<FunctionValue<'ctx>, String> {
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[self.i32_type.into(), self.i32_type.into()], false);
        let function = exported_function(self.module, "lume_init", fn_type);
        let block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(block);
        self.builder
            .build_store(
                self.patch_len.as_pointer_value(),
                self.i32_type.const_zero(),
            )
            .map_err(|err| format!("failed to initialize patch length: {err}"))?;
        self.builder
            .build_return(None)
            .map_err(|err| format!("failed to return from lume_init: {err}"))?;
        verify(function)
    }

    fn emit_dispatch(&self) -> Result<FunctionValue<'ctx>, String> {
        let fn_type = self.i32_type.fn_type(
            &[
                self.i32_type.into(),
                self.i32_type.into(),
                self.i32_type.into(),
            ],
            false,
        );
        let function = exported_function(self.module, "lume_dispatch", fn_type);
        let block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(block);
        self.builder
            .build_store(
                self.patch_len.as_pointer_value(),
                self.i32_type.const_zero(),
            )
            .map_err(|err| format!("failed to reset patch length: {err}"))?;
        self.builder
            .build_return(Some(&self.i32_type.const_zero()))
            .map_err(|err| format!("failed to return from lume_dispatch: {err}"))?;
        verify(function)
    }

    fn emit_get_patch_len(&self) -> Result<FunctionValue<'ctx>, String> {
        let fn_type = self.i32_type.fn_type(&[self.i32_type.into()], false);
        let function = exported_function(self.module, "lume_get_patch_len", fn_type);
        let block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(block);
        let len = self
            .builder
            .build_load(
                self.i32_type,
                self.patch_len.as_pointer_value(),
                "patch_len",
            )
            .map_err(|err| format!("failed to load patch length: {err}"))?
            .into_int_value();
        self.builder
            .build_return(Some(&len))
            .map_err(|err| format!("failed to return from lume_get_patch_len: {err}"))?;
        verify(function)
    }

    fn emit_alloc(&self) -> Result<FunctionValue<'ctx>, String> {
        let fn_type = self.i32_type.fn_type(&[self.i32_type.into()], false);
        let function = exported_function(self.module, "lume_alloc", fn_type);
        let block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(block);
        let size = function
            .get_nth_param(0)
            .expect("lume_alloc has a size param")
            .into_int_value();
        let cursor = self
            .builder
            .build_load(
                self.i32_type,
                self.heap_cursor.as_pointer_value(),
                "heap_cursor",
            )
            .map_err(|err| format!("failed to load heap cursor: {err}"))?
            .into_int_value();
        let aligned_size = align_to_eight(&self.builder, self.i32_type, size)?;
        let next = self
            .builder
            .build_int_add(cursor, aligned_size, "heap_next")
            .map_err(|err| format!("failed to advance heap cursor: {err}"))?;
        self.builder
            .build_store(self.heap_cursor.as_pointer_value(), next)
            .map_err(|err| format!("failed to store heap cursor: {err}"))?;
        self.builder
            .build_return(Some(&cursor))
            .map_err(|err| format!("failed to return from lume_alloc: {err}"))?;
        verify(function)
    }

    fn emit_free(&self) -> Result<FunctionValue<'ctx>, String> {
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[self.i32_type.into()], false);
        let function = exported_function(self.module, "lume_free", fn_type);
        let block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(block);
        self.builder
            .build_return(None)
            .map_err(|err| format!("failed to return from lume_free: {err}"))?;
        verify(function)
    }
}

fn exported_function<'ctx>(
    module: &Module<'ctx>,
    name: &str,
    fn_type: inkwell::types::FunctionType<'ctx>,
) -> FunctionValue<'ctx> {
    let function = module.add_function(name, fn_type, None);
    function.set_linkage(Linkage::External);
    function
}

fn align_to_eight<'ctx>(
    builder: &Builder<'ctx>,
    i32_type: IntType<'ctx>,
    value: IntValue<'ctx>,
) -> Result<IntValue<'ctx>, String> {
    let plus = builder
        .build_int_add(value, i32_type.const_int(7, false), "alloc_plus_align")
        .map_err(|err| format!("failed to build allocation alignment add: {err}"))?;
    builder
        .build_and(
            plus,
            i32_type.const_int(!7u32 as u64, false),
            "alloc_aligned",
        )
        .map_err(|err| format!("failed to build allocation alignment mask: {err}"))
}

fn initial_i32(raw: &str) -> u64 {
    raw.trim()
        .trim_matches('"')
        .parse::<i64>()
        .map(|value| value as u64)
        .unwrap_or(0)
}

fn verify(function: FunctionValue<'_>) -> Result<FunctionValue<'_>, String> {
    if function.verify(true) {
        Ok(function)
    } else {
        Err(format!(
            "LLVM verification failed for WASM export `{}`",
            function.get_name().to_string_lossy()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{emit_llvm_wasm, WasmBackend, WASM_TARGET};
    use lume_hir::lower;
    use lume_ir::build;
    use lume_parser::parse;

    #[test]
    fn skeleton_emits_valid_header_placeholder() {
        let bytes = WasmBackend::new().emit_skeleton();
        assert!(bytes.starts_with(b"\0asm\x01\0\0\0"));
        assert!(bytes
            .windows(b"lume_init".len())
            .any(|item| item == b"lume_init"));
        assert!(bytes
            .windows(b"lume_dispatch".len())
            .any(|item| item == b"lume_dispatch"));
        assert!(bytes
            .windows(b"lume_get_patch_len".len())
            .any(|item| item == b"lume_get_patch_len"));
        assert_sections_are_well_sized(bytes);
    }

    #[test]
    fn emits_wasm32_object_with_runtime_abi_from_llvm() {
        let source = r#"
component App {
  state count: i32 = 7

  view {
    Text("Count: {count}")
  }
}
"#;
        let (program, diagnostics) = parse(source);
        assert!(!diagnostics.has_errors());
        let ir = build(&lower(program)).expect("ir");
        let artifact = emit_llvm_wasm(&ir).expect("wasm artifact");

        assert_eq!(artifact.target, WASM_TARGET);
        assert!(artifact.wasm.starts_with(b"\0asm\x01\0\0\0"));
        assert!(artifact.object.starts_with(b"\0asm\x01\0\0\0"));
        assert_sections_are_well_sized(&artifact.wasm);
        assert!(artifact
            .wasm
            .windows(b"lume_alloc".len())
            .any(|item| item == b"lume_alloc"));
        assert!(artifact
            .llvm_ir
            .contains("target triple = \"wasm32-unknown-unknown\""));
        assert!(artifact.llvm_ir.contains("define void @lume_init(i32"));
        assert!(artifact.llvm_ir.contains("define i32 @lume_dispatch(i32"));
        assert!(artifact
            .llvm_ir
            .contains("define i32 @lume_get_patch_len(i32"));
        assert!(artifact.llvm_ir.contains("define i32 @lume_alloc(i32"));
        assert!(artifact.llvm_ir.contains("define void @lume_free(i32"));
        assert!(artifact.llvm_ir.contains("__lume_state_0_count"));
    }

    fn assert_sections_are_well_sized(bytes: &[u8]) {
        let mut index = 8;
        while index < bytes.len() {
            let _section_id = bytes[index];
            index += 1;
            let (size, read) = read_leb_u32(&bytes[index..]);
            index += read;
            index += size as usize;
            assert!(index <= bytes.len(), "section extends past end of module");
        }
        assert_eq!(index, bytes.len());
    }

    fn read_leb_u32(bytes: &[u8]) -> (u32, usize) {
        let mut value = 0u32;
        let mut shift = 0;
        for (index, byte) in bytes.iter().copied().enumerate() {
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return (value, index + 1);
            }
            shift += 7;
        }
        panic!("unterminated leb128 value");
    }
}
