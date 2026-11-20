# The x86 Arch rootfs of FEX-Emu (rootfs.fex-emu.gg), an EroFS image, for
# the FEX of Steam: an automount unit puts it read-only at
# /usr/share/guestos/fex-mesa, the path fex-compat-tool of Valve looks at,
# on the first access, and takes it off after 10 minutes of no use. A
# preset enables the automount unit.
# The image has graphics_provider.json in its root and mesa with v3d and
# broadcom (Vulkan) for x86_64 and i386. The version is the date of the
# image.

EAPI=8

inherit linux-info systemd

MY_DATE="${PV:0:4}-${PV:4:2}-${PV:6:2}"

DESCRIPTION="x86 Arch rootfs of FEX-Emu for the FEX of Steam, at /usr/share/guestos/fex-mesa"
HOMEPAGE="https://rootfs.fex-emu.gg/"
SRC_URI="https://rootfs.fex-emu.gg/ArchLinux/${MY_DATE}/ArchLinux.ero -> ${P}.ero"
S=${WORKDIR}

# the licenses of the packages of Arch Linux in the image
LICENSE="metapackage"
SLOT="0"
KEYWORDS="arm64"
RESTRICT="binchecks bindist mirror strip"

RDEPEND="sys-apps/systemd"

# autofs: the automount unit; loop and erofs: the image; the image is
# compressed by LZ4, which EROFS_FS_ZIP has
CONFIG_CHECK="~AUTOFS_FS ~BLK_DEV_LOOP ~EROFS_FS ~EROFS_FS_ZIP"
ERROR_AUTOFS_FS="the automount unit needs autofs"
ERROR_EROFS_FS_ZIP="the image is compressed by LZ4"

# x86_64 programs need 47 bits of virtual address or more
pkg_setup() {
	linux-info_pkg_setup
	linux_config_exists || return
	local va
	va=$(linux_chkconfig_string ARM64_VA_BITS)
	if [[ -n ${va} ]] && (( va < 47 )); then
		ewarn "CONFIG_ARM64_VA_BITS=${va}: x86_64 programs under FEX need 47 or more"
	fi
}

src_unpack() {
	:
}

src_install() {
	insinto /usr/share/fex-emu/RootFS
	newins "${DISTDIR}/${P}.ero" ArchLinux.ero
	keepdir /usr/share/guestos/fex-mesa
	systemd_dounit "${FILESDIR}"/'usr-share-guestos-fex\x2dmesa.mount' \
		"${FILESDIR}"/'usr-share-guestos-fex\x2dmesa.automount'
	insinto "$(systemd_get_systempresetdir)"
	doins "${FILESDIR}"/50-fex-rootfs-arch.preset
}

pkg_postinst() {
	elog "A preset enables the automount unit for a new system (systemctl preset)."
	elog "On this one, mount the rootfs for the FEX of Steam on demand:"
	elog "    systemctl enable --now 'usr-share-guestos-fex\\x2dmesa.automount'"
	elog "or at each boot, in place of it:"
	elog "    systemctl enable --now 'usr-share-guestos-fex\\x2dmesa.mount'"
	elog "The mesa of the image (26.1.6) has the drivers of the GPU of the"
	elog "Raspberry Pi for x86_64 and i386: v3d (OpenGL) and broadcom (Vulkan)."
	elog "A new image needs a remount: systemctl restart the unit."
}
