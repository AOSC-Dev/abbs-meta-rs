/// variables.rs - Known variables to skip the undefined variable error.

/// Known variables during the execution of autobuild.
///
/// Undefined variable errors should be ignored for these variables.
///
/// Part of this collection is extracted from [`autobuild4/lib/default-paths.sh`](file:///usr/lib/autobuild4/lib/default-paths.sh)
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
pub fn is_known_variable(v: &str) -> bool {
	KNOWN_VARIABLES.contains(&v)
}
