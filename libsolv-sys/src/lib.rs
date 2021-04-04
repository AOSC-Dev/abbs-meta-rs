//! Low-level Rust binding for libsolv.
//!
//! The current binding is tailored to dpkg-based distro usage
//! (Enabled libsolvext but only Debian support is added).
//!
//! All the bindings are inside the `ffi` module.
//! Since this is a low-level binding library, you need to consult
//! [libsolv C API documentation](https://github.com/openSUSE/libsolv) for more information.

/// FFI bindings for libsolv
pub mod ffi;
