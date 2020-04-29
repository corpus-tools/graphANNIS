#[macro_use]
extern crate log;

#[allow(unused_macros)]
macro_rules! cast_mut {
    ($x:expr) => {{
        unsafe {
            assert!(!$x.is_null());
            (&mut (*$x))
        }
    }};
}

macro_rules! cast_const {
    ($x:expr) => {{
        unsafe {
            assert!(!$x.is_null(), "Object argument was null");
            (&(*$x))
        }
    }};
}

macro_rules! cstr {
    ($x:expr) => {
        unsafe {
            use std::borrow::Cow;
            if $x.is_null() {
                Cow::from("")
            } else {
                std::ffi::CStr::from_ptr($x).to_string_lossy()
            }
        }
    };
}

macro_rules! try_cerr {
    ($x:expr , $err_ptr:ident, $default_return_val:expr) => {
        match $x {
            Ok(v) => v,
            Err(err) => {
                if !$err_ptr.is_null() {
                    unsafe {
                        *$err_ptr = cerror::new(err.into());
                    }
                }
                return $default_return_val;
            }
        }
    };
}

/// Simple definition of a matrix from a single data type.
pub type Matrix<T> = Vec<Vec<T>>;

pub mod cerror;
pub mod corpusstorage;
pub mod data;
pub mod graph;
pub mod logging;
pub mod update;
