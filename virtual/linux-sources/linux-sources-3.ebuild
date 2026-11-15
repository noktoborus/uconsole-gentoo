# virtual/linux-sources of the uConsole: it points to sys-kernel/uconsole-pi-kernel,
# which installs the tree for external modules in /usr/src/linux. A package
# of a module (linux-mod-r1) depends on virtual/linux-sources at build
# time; the virtual of the gentoo repository lists only the kernels of
# gentoo, and would pull sys-kernel/gentoo-sources. Every
# virtual/linux-sources is masked (package.mask/uconsole) but this one
# (package.unmask/uconsole).

EAPI=8

DESCRIPTION="Virtual to depend on the sources of the kernel of the uConsole"
SLOT="0"
KEYWORDS="arm64"
IUSE="firmware"

RDEPEND="
	firmware? ( sys-kernel/linux-firmware )
	sys-kernel/uconsole-pi-kernel
"
