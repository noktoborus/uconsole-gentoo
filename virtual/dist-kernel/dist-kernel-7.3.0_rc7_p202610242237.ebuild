# virtual/dist-kernel of the uConsole: it points to sys-kernel/uconsole-kernel
# of the same version. A package of a kernel module (linux-mod-r1, USE
# dist-kernel) depends on virtual/dist-kernel:=, so each new kernel has a
# new subslot here, and Portage builds those modules again with it. The
# virtual of the gentoo repository is masked (package.mask/uconsole).
# kernel/bump.sh renames it with the kernel.

EAPI=8

DESCRIPTION="Virtual to depend on the kernel of the uConsole"
SLOT="0/${PVR}"
KEYWORDS="~arm64"

RDEPEND="~sys-kernel/uconsole-kernel-${PV}"
