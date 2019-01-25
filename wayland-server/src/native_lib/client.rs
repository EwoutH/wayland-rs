use std::os::raw::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use wayland_sys::server::*;

use super::resource::NewResourceInner;
use {Interface, UserDataMap};

type BoxedDest = Box<FnMut(&UserDataMap) + Send + 'static>;

pub(crate) struct ClientInternal {
    alive: AtomicBool,
    user_data_map: UserDataMap,
    destructors: Mutex<Vec<BoxedDest>>,
}

impl ClientInternal {
    fn new() -> ClientInternal {
        ClientInternal {
            alive: AtomicBool::new(true),
            user_data_map: UserDataMap::new(),
            destructors: Mutex::new(Vec::new()),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ClientInner {
    ptr: *mut wl_client,
    internal: Arc<ClientInternal>,
}

impl ClientInner {
    pub(crate) unsafe fn from_ptr(ptr: *mut wl_client) -> ClientInner {
        // check if we are already registered
        let listener = ffi_dispatch!(
            WAYLAND_SERVER_HANDLE,
            wl_client_get_destroy_listener,
            ptr,
            client_destroy
        );
        if listener.is_null() {
            // need to init this client
            let listener = signal::rust_listener_create(client_destroy);
            let internal = Arc::new(ClientInternal::new());
            signal::rust_listener_set_user_data(
                listener,
                Box::into_raw(Box::new(internal.clone())) as *mut c_void,
            );
            ffi_dispatch!(
                WAYLAND_SERVER_HANDLE,
                wl_client_add_destroy_listener,
                ptr,
                listener
            );
            ClientInner { ptr, internal }
        } else {
            // client already initialized
            let internal = signal::rust_listener_get_user_data(listener) as *mut Arc<ClientInternal>;
            ClientInner {
                ptr,
                internal: (*internal).clone(),
            }
        }
    }

    pub(crate) fn ptr(&self) -> *mut wl_client {
        self.ptr
    }

    pub(crate) fn alive(&self) -> bool {
        self.internal.alive.load(Ordering::Acquire)
    }

    pub(crate) fn equals(&self, other: &ClientInner) -> bool {
        Arc::ptr_eq(&self.internal, &other.internal)
    }

    pub(crate) fn flush(&self) {
        if !self.alive() {
            return;
        }
        unsafe {
            ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_client_flush, self.ptr);
        }
    }

    pub(crate) fn kill(&self) {
        if !self.alive() {
            return;
        }
        unsafe {
            ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_client_destroy, self.ptr);
        }
    }

    pub(crate) fn user_data_map(&self) -> &UserDataMap {
        &self.internal.user_data_map
    }

    pub(crate) fn add_destructor<F: FnOnce(&UserDataMap) + Send + 'static>(&self, destructor: F) {
        // Wrap the FnOnce in an FnMut because Box<FnOnce()> does not work
        // currently =(
        let mut opt_dest = Some(destructor);
        self.internal
            .destructors
            .lock()
            .unwrap()
            .push(Box::new(move |data_map| {
                if let Some(dest) = opt_dest.take() {
                    dest(data_map);
                }
            }))
    }

    pub(crate) fn create_resource<I: Interface>(&self, version: u32) -> Option<NewResourceInner> {
        if !self.alive() {
            return None;
        }
        unsafe {
            let ptr = ffi_dispatch!(
                WAYLAND_SERVER_HANDLE,
                wl_resource_create,
                self.ptr(),
                I::c_interface(),
                version as i32,
                0
            );
            Some(NewResourceInner::from_c_ptr(ptr))
        }
    }
}

unsafe extern "C" fn client_destroy(listener: *mut wl_listener, _data: *mut c_void) {
    let internal = Box::from_raw(signal::rust_listener_get_user_data(listener) as *mut Arc<ClientInternal>);
    signal::rust_listener_set_user_data(listener, ptr::null_mut());
    // Store that we are dead
    internal.alive.store(false, Ordering::Release);

    let mut destructors = internal.destructors.lock().unwrap();
    for mut destructor in destructors.drain(..) {
        destructor(&internal.user_data_map);
    }

    signal::rust_listener_destroy(listener);
}
