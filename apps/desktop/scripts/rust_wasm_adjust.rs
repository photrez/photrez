#![no_main]
#![crate_type = "cdylib"]
// Minimal Rust/WASM per-pixel adjustment (C-ABI) for benchmark vs TS-CPU.
// Compile (no cargo crate needed):
//   & "C:\Users\Qolbi\.cargo\bin\rustc.exe" --target wasm32-unknown-unknown -O scripts/rust_wasm_adjust.rs -o scripts/rust_wasm_adjust.wasm
#[no_mangle]
pub extern "C" fn adjust(ptr: *mut u8, len: usize, br: f32, ct: f32) {
    unsafe {
        let sl = std::slice::from_raw_parts_mut(ptr, len);
        let mut i = 0;
        while i < len {
            sl[i] = (sl[i] as f32 * br + ct).clamp(0.0, 255.0) as u8;
            sl[i + 1] = (sl[i + 1] as f32 * br + ct).clamp(0.0, 255.0) as u8;
            sl[i + 2] = (sl[i + 2] as f32 * br + ct).clamp(0.0, 255.0) as u8;
            i += 4;
        }
    }
}

#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let mut v: Vec<u8> = Vec::with_capacity(size);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}
