# The kernel of the ClockworkPi uConsole: Raspberry Pi rpi-7.3.y with the
# series of kernel/patches/uconsole-7.3 of this project, for the platform
# UCONSOLE_PLATFORM of make.conf (cm5, cm4). Built in the builder as each
# other package; the uConsole takes its binary package.
#
# It downloads nothing. The builder mounts, read-only:
#   /usr/src/uconsole-base    the clean tree of the base (kernel/base.sh)
#   /usr/src/uconsole-kernel  kernel/ of this project: the series, the
#                             fragments of the configuration, config.sh,
#                             firmware/ (the splash built into the kernel)
# kernel/bump.sh gives it a new version when the series changes; it stops
# if the series is not the one it records.

EAPI=8

inherit toolchain-funcs

DESCRIPTION="Kernel of the ClockworkPi uConsole: Raspberry Pi rpi-7.3.y and its patches"
HOMEPAGE="https://github.com/raspberrypi/linux"
S=${WORKDIR}/linux

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="~arm64"
RESTRICT="binchecks strip test"

BDEPEND="
	app-alternatives/bc
	app-alternatives/lex
	app-alternatives/yacc
	dev-lang/perl
	dev-libs/openssl
	sys-apps/kmod
	virtual/libelf
"

# written by kernel/bump.sh
UCONSOLE_BASE=6634f17dc1348df5d3c98023c24ad9891318a600
UCONSOLE_SERIES_SHA256=f9bf4fb75c2206b8a489927de633618b0bc69b0e8a88398cda0232aeaefe2887

UCONSOLE_BASE_DIR=/usr/src/uconsole-base/linux-${UCONSOLE_BASE}
UCONSOLE_KERNEL_DIR=/usr/src/uconsole-kernel

pkg_setup() {
	case ${UCONSOLE_PLATFORM} in
		cm5) UCONSOLE_IMAGE=kernel_2712 UCONSOLE_DTBS=bcm2712 ;;
		cm4) UCONSOLE_IMAGE=kernel8 UCONSOLE_DTBS=bcm2711-rpi-cm4 ;;
		*) die "UCONSOLE_PLATFORM of make.conf is '${UCONSOLE_PLATFORM}', not cm5 or cm4" ;;
	esac
}

src_unpack() {
	[[ -f ${UCONSOLE_BASE_DIR}/Makefile ]] ||
		die "no base tree in ${UCONSOLE_BASE_DIR}: run kernel/base.sh (gentoo-builder/README.md)"
	[[ -d ${UCONSOLE_KERNEL_DIR}/patches/uconsole-7.3 ]] ||
		die "no ${UCONSOLE_KERNEL_DIR}: mount kernel/ of the project there"
	mkdir -p "${S}" || die
	cp -a "${UCONSOLE_BASE_DIR}/." "${S}/" || die
}

src_prepare() {
	local sum
	sum=$(cat "${UCONSOLE_KERNEL_DIR}"/patches/uconsole-7.3/*.patch \
		"${UCONSOLE_KERNEL_DIR}"/uconsole*.config \
		"${UCONSOLE_KERNEL_DIR}"/firmware/*.tga | sha256sum) || die
	[[ ${sum%% *} == "${UCONSOLE_SERIES_SHA256}" ]] ||
		die "the series, the fragments or firmware/ changed: run kernel/bump.sh"

	eapply "${UCONSOLE_KERNEL_DIR}"/patches/uconsole-7.3/*.patch
	eapply_user
}

src_configure() {
	# The kernel takes no CFLAGS of make.conf: its own flags, and the
	# compiler of the toolchain (ccache and distcc through the path).
	unset KBUILD_OUTPUT CFLAGS CXXFLAGS LDFLAGS
	UCONSOLE_MAKEARGS=(
		ARCH=arm64
		CC="$(tc-getCC)" HOSTCC="$(tc-getBUILD_CC)"
		LD="$(tc-getLD)" AR="$(tc-getAR)" NM="$(tc-getNM)"
		OBJCOPY="$(tc-getOBJCOPY)" STRIP="$(tc-getSTRIP)"
	)
	KMAKE="make ${MAKEOPTS} ${UCONSOLE_MAKEARGS[*]}" \
		"${UCONSOLE_KERNEL_DIR}"/config.sh "${S}" "${WORKDIR}/build" "${UCONSOLE_PLATFORM}" ||
		die "config.sh failed"
}

src_compile() {
	emake -C "${S}" O="${WORKDIR}/build" "${UCONSOLE_MAKEARGS[@]}" Image modules dtbs
}

src_install() {
	local b=${WORKDIR}/build
	local kv=$(<"${b}/include/config/kernel.release")
	local kdir=/usr/src/linux-${kv}

	# /boot/firmware: the kernel under the name the firmware looks for,
	# the device trees of the platform, the overlays
	insinto /boot/firmware
	newins "${b}/arch/arm64/boot/Image" "${UCONSOLE_IMAGE}.img"
	doins "${b}"/arch/arm64/boot/dts/broadcom/${UCONSOLE_DTBS}*.dtb
	insinto /boot/firmware/overlays
	doins "${b}"/arch/arm64/boot/dts/overlays/*.dtbo "${S}/arch/arm/boot/dts/overlays/README"

	emake -C "${S}" O="${b}" "${UCONSOLE_MAKEARGS[@]}" \
		INSTALL_MOD_PATH="${ED}" INSTALL_MOD_STRIP=1 modules_install
	rm -f "${ED}/lib/modules/${kv}"/{build,source} || die

	# The tree for external modules (linux-mod-r1): what the kernel itself
	# gives for that, as kernel.spec of the kernel does
	emake -C "${S}" O="${b}" "${UCONSOLE_MAKEARGS[@]}" run-command \
		KBUILD_RUN_COMMAND="${S}/scripts/package/install-extmod-build ${ED}${kdir}"
	insinto "${kdir}"
	doins "${b}/.config" "${b}/System.map"
	dosym "../../..${kdir}" "/lib/modules/${kv}/build"
	dosym "linux-${kv}" /usr/src/linux
}
