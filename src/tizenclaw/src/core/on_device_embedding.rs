//! On-device embedding using all-MiniLM-L6-v2 ONNX model.
//!
//! Loads ONNX Runtime via `dlopen` for graceful fallback.
//! Generates 384-dim embeddings independently of LLM backend.

#![allow(clippy::all)]

use super::wordpiece_tokenizer::WordPieceTokenizer;

/// Embedding dimension for all-MiniLM-L6-v2.
pub const EMBEDDING_DIM: usize = 384;

/// Default path to ONNX Runtime shared library.
/// Falls back to env-based path or standard Tizen location.
const DEFAULT_ORT_LIB_PATH: &str =
    "/usr/lib/libonnxruntime.so";

// ═══════════════════════════════════════════
//  ORT C API types (minimal set for dlopen)
// ═══════════════════════════════════════════

#[repr(C)]
struct OrtApiBase {
    get_api:
        unsafe extern "C" fn(version: u32) -> *const OrtApi,
    get_version_string: unsafe extern "C" fn() -> *const libc::c_char,
}

// Opaque handles
#[repr(C)]
struct OrtEnv { _opaque: [u8; 0] }
#[repr(C)]
struct OrtSessionOptions { _opaque: [u8; 0] }
#[repr(C)]
struct OrtSession { _opaque: [u8; 0] }
#[repr(C)]
struct OrtAllocator { _opaque: [u8; 0] }
#[repr(C)]
struct OrtMemoryInfo { _opaque: [u8; 0] }
#[repr(C)]
struct OrtValue { _opaque: [u8; 0] }
#[repr(C)]
struct OrtStatus { _opaque: [u8; 0] }
#[repr(C)]
struct OrtRunOptions { _opaque: [u8; 0] }

// ORT logging level
const ORT_LOGGING_LEVEL_WARNING: i32 = 2;
// ORT graph optimization level
const ORT_ENABLE_ALL: i32 = 99;
// ORT allocator type
const ORT_ARENA_ALLOCATOR: i32 = 0;
// ORT memory type
const ORT_MEM_TYPE_DEFAULT: i32 = 0;
// ORT tensor element type
const ONNX_TENSOR_ELEMENT_DATA_TYPE_INT64: i32 = 7;

/// The ORT C API struct — we only declare the function pointers we need.
/// In the real header this is a massive vtable; we index by offset.
/// For safety, we use a raw approach: treat the API as a table of fn pointers.
#[repr(C)]
struct OrtApi {
    // We store the raw pointer and call through offset-based accessors.
    // This approach avoids having to declare 200+ fn pointers.
    _pad: [u8; 0],
}

/// Wrapper that holds resolved ORT function pointers.
struct OrtFunctions {
    create_env: unsafe extern "C" fn(i32, *const libc::c_char, *mut *mut OrtEnv) -> *mut OrtStatus,
    create_session_options: unsafe extern "C" fn(*mut *mut OrtSessionOptions) -> *mut OrtStatus,
    set_intra_op_num_threads: unsafe extern "C" fn(*mut OrtSessionOptions, i32) -> *mut OrtStatus,
    set_inter_op_num_threads: unsafe extern "C" fn(*mut OrtSessionOptions, i32) -> *mut OrtStatus,
    set_session_graph_optimization_level: unsafe extern "C" fn(*mut OrtSessionOptions, i32) -> *mut OrtStatus,
    disable_cpu_mem_arena: unsafe extern "C" fn(*mut OrtSessionOptions) -> *mut OrtStatus,
    disable_mem_pattern: unsafe extern "C" fn(*mut OrtSessionOptions) -> *mut OrtStatus,
    create_session: unsafe extern "C" fn(*mut OrtEnv, *const libc::c_char, *const OrtSessionOptions, *mut *mut OrtSession) -> *mut OrtStatus,
    get_allocator_with_default_options: unsafe extern "C" fn(*mut *mut OrtAllocator) -> *mut OrtStatus,
    create_cpu_memory_info: unsafe extern "C" fn(i32, i32, *mut *mut OrtMemoryInfo) -> *mut OrtStatus,
    create_tensor_with_data: unsafe extern "C" fn(*const OrtMemoryInfo, *mut libc::c_void, usize, *const i64, usize, i32, *mut *mut OrtValue) -> *mut OrtStatus,
    run: unsafe extern "C" fn(*mut OrtSession, *const OrtRunOptions, *const *const libc::c_char, *const *const OrtValue, usize, *const *const libc::c_char, usize, *mut *mut OrtValue) -> *mut OrtStatus,
    get_tensor_mutable_data: unsafe extern "C" fn(*mut OrtValue, *mut *mut libc::c_void) -> *mut OrtStatus,
    release_env: unsafe extern "C" fn(*mut OrtEnv),
    release_session: unsafe extern "C" fn(*mut OrtSession),
    release_session_options: unsafe extern "C" fn(*mut OrtSessionOptions),
    release_value: unsafe extern "C" fn(*mut OrtValue),
    release_memory_info: unsafe extern "C" fn(*mut OrtMemoryInfo),
    release_status: unsafe extern "C" fn(*mut OrtStatus),
    get_error_message: unsafe extern "C" fn(*const OrtStatus) -> *const libc::c_char,
}

// ═══════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════

pub struct OnDeviceEmbedding {
    tokenizer: WordPieceTokenizer,
    ort_lib: *mut libc::c_void,
    env: *mut OrtEnv,
    session: *mut OrtSession,
    session_options: *mut OrtSessionOptions,
    fns: Option<OrtFunctions>,
}

// Safety: the ORT handles are not Send/Sync by default but we only use them
// from a single thread (the embedding encode path is mutex-guarded in agent_core).
unsafe impl Send for OnDeviceEmbedding {}
unsafe impl Sync for OnDeviceEmbedding {}

impl Default for OnDeviceEmbedding {
    fn default() -> Self {
        Self::new()
    }
}

impl OnDeviceEmbedding {
    pub fn new() -> Self {
        Self {
            tokenizer: WordPieceTokenizer::new(),
            ort_lib: std::ptr::null_mut(),
            env: std::ptr::null_mut(),
            session: std::ptr::null_mut(),
            session_options: std::ptr::null_mut(),
            fns: None,
        }
    }

    /// Initialize: load ONNX Runtime, model, and vocab.
    /// `model_dir` should contain `model.onnx` and `vocab.txt`.
    pub fn initialize(&mut self, model_dir: &str, ort_lib_path: Option<&str>) -> bool {
        let ort_path = ort_lib_path.unwrap_or(DEFAULT_ORT_LIB_PATH);

        // 1. Load ONNX Runtime via dlopen
        let lib = unsafe {
            libc::dlopen(
                std::ffi::CString::new(ort_path).unwrap().as_ptr(),
                libc::RTLD_LAZY,
            )
        };
        if lib.is_null() {
            let err = unsafe {
                let e = libc::dlerror();
                if e.is_null() { "unknown".to_string() }
                else { std::ffi::CStr::from_ptr(e).to_string_lossy().to_string() }
            };
            log::warn!("ONNX Runtime not found: {} (on-device embedding disabled)", err);
            return false;
        }
        self.ort_lib = lib;

        // 2. Get OrtGetApiBase
        let get_api_base_sym = unsafe {
            libc::dlsym(
                lib,
                b"OrtGetApiBase\0".as_ptr() as *const libc::c_char,
            )
        };
        if get_api_base_sym.is_null() {
            log::error!("OrtGetApiBase not found");
            self.shutdown();
            return false;
        }

        type GetApiBaseFn = unsafe extern "C" fn() -> *const OrtApiBase;
        let get_api_base: GetApiBaseFn = unsafe { std::mem::transmute(get_api_base_sym) };
        let api_base = unsafe { get_api_base() };
        if api_base.is_null() {
            log::error!("OrtGetApiBase returned null");
            self.shutdown();
            return false;
        }

        // Get API version 18 (compatible with ORT 1.20)
        let api = unsafe { ((*api_base).get_api)(18) };
        if api.is_null() {
            log::error!("ORT API not available");
            self.shutdown();
            return false;
        }

        let version = unsafe {
            let v = ((*api_base).get_version_string)();
            if v.is_null() { "unknown".to_string() }
            else { std::ffi::CStr::from_ptr(v).to_string_lossy().to_string() }
        };
        log::info!("ONNX Runtime loaded: {}", version);

        // 3. Resolve required API functions from the vtable
        // The ORT API is a flat struct of function pointers.
        // We resolve them by reading the raw memory at known offsets.
        let fns = unsafe { self.resolve_api_functions(api) };
        if fns.is_none() {
            log::error!("Failed to resolve ORT API functions");
            self.shutdown();
            return false;
        }
        self.fns = fns;
        let f = self.fns.as_ref().unwrap();

        // 4. Create environment
        let name = std::ffi::CString::new("tizenclaw").unwrap();
        let mut env: *mut OrtEnv = std::ptr::null_mut();
        let status = unsafe { (f.create_env)(ORT_LOGGING_LEVEL_WARNING, name.as_ptr(), &mut env) };
        if !self.check_status(status) { self.shutdown(); return false; }
        self.env = env;

        // 5. Create session options
        let mut opts: *mut OrtSessionOptions = std::ptr::null_mut();
        let status = unsafe { (f.create_session_options)(&mut opts) };
        if !self.check_status(status) { self.shutdown(); return false; }
        self.session_options = opts;

        // Optimize for inference
        unsafe {
            (f.set_intra_op_num_threads)(opts, 2);
            (f.set_inter_op_num_threads)(opts, 1);
            (f.set_session_graph_optimization_level)(opts, ORT_ENABLE_ALL);
            (f.disable_cpu_mem_arena)(opts);
            (f.disable_mem_pattern)(opts);
        }

        // 6. Load model
        let model_path = format!("{}/model.onnx", model_dir);
        let model_cstr = std::ffi::CString::new(model_path.as_str()).unwrap();
        let mut session: *mut OrtSession = std::ptr::null_mut();
        let status = unsafe {
            (f.create_session)(env, model_cstr.as_ptr(), opts, &mut session)
        };
        if !self.check_status(status) { self.shutdown(); return false; }
        self.session = session;

        // 7. Load tokenizer vocabulary
        let vocab_path = format!("{}/vocab.txt", model_dir);
        if !self.tokenizer.load_vocab(&vocab_path) {
            log::error!("Failed to load vocab: {}", vocab_path);
            self.shutdown();
            return false;
        }

        log::info!("OnDeviceEmbedding initialized (dim={})", EMBEDDING_DIM);
        true
    }

    pub fn shutdown(&mut self) {
        if let Some(ref f) = self.fns {
            if !self.session.is_null() {
                unsafe { (f.release_session)(self.session); }
                self.session = std::ptr::null_mut();
            }
            if !self.session_options.is_null() {
                unsafe { (f.release_session_options)(self.session_options); }
                self.session_options = std::ptr::null_mut();
            }
            if !self.env.is_null() {
                unsafe { (f.release_env)(self.env); }
                self.env = std::ptr::null_mut();
            }
        }
        self.fns = None;
        // Don't dlclose — ORT may still have internal references
        self.ort_lib = std::ptr::null_mut();
    }

    /// Check if ONNX Runtime is available and model loaded.
    pub fn is_available(&self) -> bool {
        !self.session.is_null()
    }

    /// Generate embedding for text (384-dim vector).
    pub fn encode(&self, text: &str) -> Vec<f32> {
        if self.session.is_null() || self.fns.is_none() || text.is_empty() {
            return Vec::new();
        }
        let f = self.fns.as_ref().unwrap();

        // 1. Tokenize
        let tokens = self.tokenizer.tokenize(text, 128);
        let seq_len = tokens.input_ids.len() as i64;

        // 2. Create memory info for CPU
        let mut mem_info: *mut OrtMemoryInfo = std::ptr::null_mut();
        let status = unsafe {
            (f.create_cpu_memory_info)(ORT_ARENA_ALLOCATOR, ORT_MEM_TYPE_DEFAULT, &mut mem_info)
        };
        if !self.check_status(status) { return Vec::new(); }

        // 3. Create input tensors
        let shape = [1i64, seq_len];
        let data_size = (seq_len as usize) * std::mem::size_of::<i64>();

        let mut input_ids_tensor: *mut OrtValue = std::ptr::null_mut();
        let mut attention_mask_tensor: *mut OrtValue = std::ptr::null_mut();
        let mut token_type_ids_tensor: *mut OrtValue = std::ptr::null_mut();

        let create_tensor = |data: &[i64], out: &mut *mut OrtValue| -> bool {
            let status = unsafe {
                (f.create_tensor_with_data)(
                    mem_info,
                    data.as_ptr() as *mut libc::c_void,
                    data_size,
                    shape.as_ptr(),
                    2,
                    ONNX_TENSOR_ELEMENT_DATA_TYPE_INT64,
                    out,
                )
            };
            self.check_status(status)
        };

        if !create_tensor(&tokens.input_ids, &mut input_ids_tensor)
            || !create_tensor(&tokens.attention_mask, &mut attention_mask_tensor)
            || !create_tensor(&tokens.token_type_ids, &mut token_type_ids_tensor)
        {
            unsafe {
                if !input_ids_tensor.is_null() { (f.release_value)(input_ids_tensor); }
                if !attention_mask_tensor.is_null() { (f.release_value)(attention_mask_tensor); }
                if !token_type_ids_tensor.is_null() { (f.release_value)(token_type_ids_tensor); }
                (f.release_memory_info)(mem_info);
            }
            return Vec::new();
        }

        // 4. Run inference
        let input_names = [
            b"input_ids\0".as_ptr() as *const libc::c_char,
            b"attention_mask\0".as_ptr() as *const libc::c_char,
            b"token_type_ids\0".as_ptr() as *const libc::c_char,
        ];
        let output_names = [
            b"last_hidden_state\0".as_ptr() as *const libc::c_char,
        ];
        let inputs = [
            input_ids_tensor as *const OrtValue,
            attention_mask_tensor as *const OrtValue,
            token_type_ids_tensor as *const OrtValue,
        ];
        let mut output: *mut OrtValue = std::ptr::null_mut();

        let status = unsafe {
            (f.run)(
                self.session,
                std::ptr::null(),
                input_names.as_ptr(),
                inputs.as_ptr(),
                3,
                output_names.as_ptr(),
                1,
                &mut output,
            )
        };

        // Cleanup inputs
        unsafe {
            (f.release_value)(input_ids_tensor);
            (f.release_value)(attention_mask_tensor);
            (f.release_value)(token_type_ids_tensor);
            (f.release_memory_info)(mem_info);
        }

        if !self.check_status(status) { return Vec::new(); }

        // 5. Get output data
        let mut output_data: *mut libc::c_void = std::ptr::null_mut();
        let status = unsafe { (f.get_tensor_mutable_data)(output, &mut output_data) };
        if !self.check_status(status) || output_data.is_null() {
            unsafe { (f.release_value)(output); }
            return Vec::new();
        }

        // 6. Mean pooling with attention mask
        let output_floats = unsafe {
            std::slice::from_raw_parts(
                output_data as *const f32,
                seq_len as usize * EMBEDDING_DIM,
            )
        };
        let mut embedding = Self::mean_pooling(
            output_floats,
            seq_len as usize,
            EMBEDDING_DIM,
            &tokens.attention_mask,
        );

        // 7. L2 normalize
        Self::l2_normalize(&mut embedding);

        unsafe { (f.release_value)(output); }
        embedding
    }

    // ─── Private helpers ────────────────────────

    fn check_status(&self, status: *mut OrtStatus) -> bool {
        if status.is_null() {
            return true;
        }
        if let Some(ref f) = self.fns {
            let msg = unsafe {
                let p = (f.get_error_message)(status);
                if p.is_null() { "unknown".to_string() }
                else { std::ffi::CStr::from_ptr(p).to_string_lossy().to_string() }
            };
            log::error!("ORT error: {}", msg);
            unsafe { (f.release_status)(status); }
        }
        false
    }

    /// Resolve ORT API function pointers from the vtable.
    ///
    /// The ORT API is a C struct of ~200 function pointers laid out sequentially.
    /// We read at the documented offsets for the functions we need.
    unsafe fn resolve_api_functions(&self, api: *const OrtApi) -> Option<OrtFunctions> {
        // The ORT C API struct is a flat array of function pointers.
        // Offsets are determined by the order in onnxruntime_c_api.h
        // We treat the API pointer as an array of function pointers.
        let table = api as *const *const libc::c_void;

        // These offsets correspond to ORT API v18 (1.20.x)
        // Verified against onnxruntime_c_api.h
        Some(OrtFunctions {
            create_env: std::mem::transmute(*table.add(1)),          // CreateEnv
            create_session_options: std::mem::transmute(*table.add(10)),  // CreateSessionOptions
            set_intra_op_num_threads: std::mem::transmute(*table.add(5)),  // SetIntraOpNumThreads
            set_inter_op_num_threads: std::mem::transmute(*table.add(65)), // SetInterOpNumThreads
            set_session_graph_optimization_level: std::mem::transmute(*table.add(12)), // SetSessionGraphOptimizationLevel
            disable_cpu_mem_arena: std::mem::transmute(*table.add(14)), // DisableCpuMemArena
            disable_mem_pattern: std::mem::transmute(*table.add(16)),  // DisableMemPattern
            create_session: std::mem::transmute(*table.add(2)),       // CreateSession
            get_allocator_with_default_options: std::mem::transmute(*table.add(18)), // GetAllocatorWithDefaultOptions
            create_cpu_memory_info: std::mem::transmute(*table.add(21)), // CreateCpuMemoryInfo
            create_tensor_with_data: std::mem::transmute(*table.add(22)), // CreateTensorWithDataAsOrtValue
            run: std::mem::transmute(*table.add(9)),                  // Run
            get_tensor_mutable_data: std::mem::transmute(*table.add(23)), // GetTensorMutableData
            release_env: std::mem::transmute(*table.add(42)),         // ReleaseEnv
            release_session: std::mem::transmute(*table.add(38)),     // ReleaseSession
            release_session_options: std::mem::transmute(*table.add(39)), // ReleaseSessionOptions
            release_value: std::mem::transmute(*table.add(41)),       // ReleaseValue
            release_memory_info: std::mem::transmute(*table.add(43)), // ReleaseMemoryInfo
            release_status: std::mem::transmute(*table.add(4)),       // ReleaseStatus
            get_error_message: std::mem::transmute(*table.add(3)),    // GetErrorMessage
        })
    }

    fn mean_pooling(output: &[f32], seq_len: usize, hidden_dim: usize, attn_mask: &[i64]) -> Vec<f32> {
        let mut result = vec![0.0f32; hidden_dim];
        let mut mask_sum = 0.0f32;

        for i in 0..seq_len {
            let mask = attn_mask[i] as f32;
            mask_sum += mask;
            for j in 0..hidden_dim {
                result[j] += output[i * hidden_dim + j] * mask;
            }
        }

        if mask_sum > 0.0 {
            for j in 0..hidden_dim {
                result[j] /= mask_sum;
            }
        }

        result
    }

    fn l2_normalize(vec: &mut [f32]) {
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 1e-12 {
            for v in vec.iter_mut() {
                *v /= norm;
            }
        }
    }
}

impl Drop for OnDeviceEmbedding {
    fn drop(&mut self) {
        self.shutdown();
    }
}
