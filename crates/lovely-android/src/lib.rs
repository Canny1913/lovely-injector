mod lualib;

use lovely_core::config;
use lovely_core::sys::LuaState;
use lualib::LUA_LIBRARY;
use std::path::{Path, PathBuf};
use std::{ffi::c_void, mem, panic, sync::{LazyLock, OnceLock}};
use std::process::exit;
use catch_panic::catch_panic;
use jni::{JNIEnv, JNIVersion, JavaVM};
use jni::objects::JString;
use jni::sys::{jint, jvalue, JNI_ERR};

use lovely_core::Lovely;

static RUNTIME: OnceLock<Lovely> = OnceLock::new();

static RECALL: LazyLock<
    unsafe extern "C" fn(*mut LuaState, *const u8, usize, *const u8, *const u8) -> u32,
> = LazyLock::new(|| unsafe {
    let lua_loadbufferx: unsafe extern "C" fn(
        *mut LuaState,
        *const u8,
        usize,
        *const u8,
        *const u8,
    ) -> u32 = *LUA_LIBRARY.get(b"luaL_loadbufferx").unwrap();
    let orig = dobby_rs::hook(
        lua_loadbufferx as *mut c_void,
        lua_loadbufferx_detour as *mut c_void,
    )
    .unwrap();
    mem::transmute(orig)
});

unsafe extern "C" fn lua_loadbufferx_detour(
    state: *mut LuaState,
    buf_ptr: *const u8,
    size: usize,
    name_ptr: *const u8,
    mode_ptr: *const u8,
) -> u32 {
    let result = panic::catch_unwind(|| {
        let rt = RUNTIME.get().unwrap_unchecked();
        rt.apply_buffer_patches(state, buf_ptr, size, name_ptr, mode_ptr)
    });
    result.unwrap_or_else(|e| {
        log::error!("Failed to load buffer: {}", e.downcast::<String>().unwrap_or_default());
        exit(0)
    })

}

#[no_mangle]
#[allow(non_snake_case)]
unsafe extern "C" fn luaL_loadbuffer(
    state: *mut LuaState,
    buf_ptr: *const u8,
    size: usize,
    name_ptr: *const u8,
) -> u32 {
    let result = panic::catch_unwind(|| {
        let rt = RUNTIME.get().unwrap_unchecked();
        rt.apply_buffer_patches(state, buf_ptr, size, name_ptr, std::ptr::null())
    });
    result.unwrap_or_else(|e| {
        log::error!("Failed to load buffer: {}", e.downcast::<String>().unwrap_or_default());
        exit(0)
    })
}

unsafe fn get_external_files_dir(env: &mut JNIEnv) -> Result<PathBuf, jni::errors::Error> {

    let environment_cls = env.find_class("android/os/Environment")?;

    //todo: add perm check
    let internal_storage_dir_obj= env.call_static_method(&environment_cls, "getExternalStorageDirectory", "()Ljava/io/File;", &[])?.l()?;
    let internal_storage_dir_jstr: JString = env.call_method(internal_storage_dir_obj, "getAbsolutePath", "()Ljava/lang/String;", &[])?.l()?.into();
    let internal_storage_dir_str = env.get_string(&internal_storage_dir_jstr)?;
    let path = format!("{0}/{1}", internal_storage_dir_str.to_str().unwrap(), "Balatro");
    Ok(PathBuf::from(path))
}

#[allow(non_snake_case)]
#[no_mangle]
unsafe extern "C" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
    let env = vm.get_env().unwrap();

    init(env);

    JNIVersion::V6.into()
}
#[catch_panic]
fn init(mut env: JNIEnv) {
    unsafe {
        let external_files_dir = get_external_files_dir(&mut env).expect("Failed to get external files directory.");
        let config = config::LovelyConfig {
            dump_all: false,
            vanilla: false,
            mod_dir: Some(external_files_dir.join("mods")),
        };

        let rt = Lovely::init(&|a, b, c, d, e| RECALL(a, b, c, d, e), lualib::get_lualib(), config);
        RUNTIME
            .set(rt)
            .unwrap_or_else(|_| panic!("Failed to instantiate runtime."));

        let lua_loadbuffer: unsafe extern "C" fn(
            *mut LuaState,
            *const u8,
            isize,
            *const u8,
        ) -> u32 = *LUA_LIBRARY.get(b"luaL_loadbuffer").unwrap();

        let _ = dobby_rs::hook(
            lua_loadbuffer as *mut c_void,
            luaL_loadbuffer as *mut c_void,
        )
            .unwrap();
    }
}
