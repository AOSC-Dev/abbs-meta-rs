# `libsolv-sys`

Low-level Rust binding for [`libsolv`](https://github.com/openSUSE/libsolv).

The current binding is tailored to dpkg-based distro usage (Enabled `libsolvext` but only Debian support is added).

However, the library will prefer to use the system `libsolv` if there is one. Make sure to install the development files from the package manager.
