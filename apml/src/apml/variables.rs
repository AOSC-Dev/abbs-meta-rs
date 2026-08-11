/// variables.rs - Variables that are provided by autobuild at runtime.
///
/// These variables are not defined by `spec` / `defines` files themselves but
/// are injected by the autobuild framework before the files are sourced.
///
/// Since undefined variables expand to an empty string (Bash semantics), the
/// parser no longer needs this list to suppress errors. It is retained for
/// diagnostics and documentation purposes only.
#[allow(dead_code)]
const KNOWN_VARIABLES: &[&str] = &[
	// Standard variables
	"PWD",
	"ABHOST",
	"ABBUILD",
	"ARCH",
	"DPKG_ARCH",
	// Various build directories
	"SRCDIR",
	"PKGDIR",
	"BLDDIR",
	// Predefined standard paths
	"TMPDIR",
	"PREFIX",
	"BINDIR",
	"LIBDIR",
	"SYSCONF",
	"CONFD",
	"ETCDEF",
	"LDSOCONF",
	"FCCONF",
	"LOGROT",
	"CROND",
	"SKELDIR",
	"BINFMTD",
	"X11CONF",
	"STATDIR",
	"INCLUDE",
	"BOOTDIR",
	"LIBEXEC",
	"MANDIR",
	"FDOAPP",
	"FDOICO",
	"FONTDIR",
	"USRSRC",
	"VARLIB",
	"RUNDIR",
	"DOCDIR",
	"LICDIR",
	"SYDDIR",
	"SYDSCR",
	"TMPFILE",
	"PAMDIR",
	"JAVAMOD",
	"JAVAHOME",
	"GTKDOC",
	"GSCHEMAS",
	"THEMES",
	"BASHCOMP",
	"ZSHCOMP",
	"PROFILED",
	"LOCALES",
	"VIMDIR",
	"QT4DIR",
	"QT5DIR",
	"QT4BIN",
	"QT5BIN",
	// Various compiler flags
	"CFLAGS",
	"CXXFLAGS",
	"OBJCFLAGS",
	"OBJCXXFLAGS",
	"ASFLAGS",
	"CPPFLAGS",
	"LDFLAGS",
	"RUSTFLAGS",
	// Various build-time variables
	"ABMK",
];

/// Returns `true` if the given string is in the known variables list.
#[allow(dead_code)]
pub fn is_known_variable(v: &str) -> bool {
	KNOWN_VARIABLES.contains(&v)
}
