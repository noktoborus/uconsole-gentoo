# The x86 Arch rootfs of FEX-Emu (rootfs.fex-emu.gg), an EroFS image, for
# the FEX of Steam: a mount unit puts it read-only at
# /usr/share/guestos/fex-mesa, the path fex-compat-tool of Valve looks at.
# The image has graphics_provider.json in its root and mesa with v3d and
# broadcom (Vulkan) for x86_64 and i386. The version is the date of the
# image.

EAPI=8

inherit systemd

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

src_unpack() {
	:
}

src_install() {
	insinto /usr/share/fex-emu/RootFS
	newins "${DISTDIR}/${P}.ero" ArchLinux.ero
	keepdir /usr/share/guestos/fex-mesa
	systemd_dounit "${FILESDIR}"/'usr-share-guestos-fex\x2dmesa.mount'
}

pkg_postinst() {
	elog "Mount the rootfs for the FEX of Steam:"
	elog "    systemctl enable --now 'usr-share-guestos-fex\\x2dmesa.mount'"
	elog "It needs a kernel with EROFS_FS, and x86_64 programs need 47 bits"
	elog "of virtual address or more (CONFIG_ARM64_VA_BITS)."
	elog "The mesa of the image (26.1.6) has the drivers of the GPU of the"
	elog "Raspberry Pi for x86_64 and i386: v3d (OpenGL) and broadcom (Vulkan)."
	elog "A new image needs a remount: systemctl restart the unit."
}
