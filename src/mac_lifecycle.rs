//! Route AppKit quit/reopen requests without replacing FLTK's file-open delegate.
#![cfg(target_os = "macos")]
use crate::{Action, Message};
use fltk::{app, prelude::*};
use std::{
    cell::Cell,
    ffi::{c_char, c_void},
};
type Obj = *mut c_void;
#[cfg(target_arch = "aarch64")]
type NativeBool = bool;
#[cfg(not(target_arch = "aarch64"))]
type NativeBool = i8;
thread_local! { static TX: Cell<Option<app::Sender<Message>>> = const { Cell::new(None) }; static PENDING: Cell<bool> = const {Cell::new(false)}; }
#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> Obj;
    fn sel_registerName(name: *const c_char) -> Obj;
    fn objc_msgSend();
    fn object_getClass(object: Obj) -> Obj;
    fn objc_allocateClassPair(parent: Obj, name: *const c_char, extra: usize) -> Obj;
    fn class_addMethod(
        class: Obj,
        selector: Obj,
        implementation: *const c_void,
        types: *const c_char,
    ) -> bool;
    fn objc_registerClassPair(class: Obj);
    fn object_setClass(object: Obj, class: Obj) -> Obj;
}
#[link(name = "AppKit", kind = "framework")]
extern "C" {}
extern "C" fn terminate(_: Obj, _: Obj, _: Obj) -> usize {
    crate::diagnostics::event("app", "macos_native_quit_requested");
    if PENDING.with(|p| p.replace(true)) {
        return 2;
    }
    if let Some(mut modal) = app::modal() {
        modal.hide();
    }
    TX.with(|tx| {
        if let Some(tx) = tx.get() {
            tx.send((0, Action::QuitAll));
        }
    });
    // NSTerminateLater: reply only after saving work, or explicitly cancel.
    2
}
extern "C" fn reopen(_: Obj, _: Obj, _: Obj, _: NativeBool) -> NativeBool {
    crate::diagnostics::event("app", "macos_dock_reopen_requested");
    TX.with(|tx| {
        if let Some(tx) = tx.get() {
            tx.send((0, Action::ShowWindows));
        }
    });
    #[cfg(target_arch = "aarch64")]
    {
        true
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        1
    }
}
pub fn install(tx: app::Sender<Message>) -> Result<(), &'static str> {
    unsafe {
        let send: unsafe extern "C" fn(Obj, Obj) -> Obj =
            std::mem::transmute(objc_msgSend as *const ());
        let class = objc_getClass(c"NSApplication".as_ptr());
        let application = send(class, sel_registerName(c"sharedApplication".as_ptr()));
        let delegate = send(application, sel_registerName(c"delegate".as_ptr()));
        if delegate.is_null() {
            return Err("AppKit application delegate is unavailable");
        }
        let subclass = objc_allocateClassPair(
            object_getClass(delegate),
            c"EditorToolLifecycleDelegate156".as_ptr(),
            0,
        );
        if subclass.is_null() {
            return Err("Could not install AppKit lifecycle delegate");
        }
        if !class_addMethod(
            subclass,
            sel_registerName(c"applicationShouldTerminate:".as_ptr()),
            terminate as *const c_void,
            c"Q@:@".as_ptr(),
        ) || !class_addMethod(
            subclass,
            sel_registerName(c"applicationShouldHandleReopen:hasVisibleWindows:".as_ptr()),
            reopen as *const c_void,
            if cfg!(target_arch = "aarch64") {
                c"B@:@B".as_ptr()
            } else {
                c"c@:@c".as_ptr()
            },
        ) {
            return Err("Could not register AppKit lifecycle callbacks");
        }
        objc_registerClassPair(subclass);
        TX.with(|slot| slot.set(Some(tx)));
        object_setClass(delegate, subclass);
    }
    Ok(())
}

pub fn complete(accepted: bool) {
    if !PENDING.with(|pending| pending.replace(false)) {
        return;
    }
    unsafe {
        let send: unsafe extern "C" fn(Obj, Obj) -> Obj =
            std::mem::transmute(objc_msgSend as *const ());
        let app = send(
            objc_getClass(c"NSApplication".as_ptr()),
            sel_registerName(c"sharedApplication".as_ptr()),
        );
        let reply: unsafe extern "C" fn(Obj, Obj, NativeBool) =
            std::mem::transmute(objc_msgSend as *const ());
        #[cfg(target_arch = "aarch64")]
        let value = accepted;
        #[cfg(not(target_arch = "aarch64"))]
        let value = if accepted { 1 } else { 0 };
        reply(
            app,
            sel_registerName(c"replyToApplicationShouldTerminate:".as_ptr()),
            value,
        );
    }
}
