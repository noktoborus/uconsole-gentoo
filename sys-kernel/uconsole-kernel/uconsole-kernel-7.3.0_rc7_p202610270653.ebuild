# The kernel of the ClockworkPi uConsole: Raspberry Pi rpi-7.3.y with the
# series of files/patches/uconsole-7.3, for the platform of USE (cm5, cm4). Built in the builder as each other package; the
# uConsole takes its binary package.
#
# It needs nothing but this directory: the source is the tarball of one
# commit of Raspberry Pi, files/ has the series, the fragments of the
# configuration and the splash built into the kernel. kernel/bump.sh gives
# it a new version when files/ or the commit changes.

EAPI=8

inherit toolchain-funcs

DESCRIPTION="Kernel of the ClockworkPi uConsole: Raspberry Pi rpi-7.3.y and its patches"
HOMEPAGE="https://github.com/raspberrypi/linux"

UCONSOLE_BASE=6634f17dc1348df5d3c98023c24ad9891318a600
SRC_URI="https://github.com/raspberrypi/linux/archive/${UCONSOLE_BASE}.tar.gz -> raspberrypi-linux-${UCONSOLE_BASE}.tar.gz"
S=${WORKDIR}/linux-${UCONSOLE_BASE}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="~arm64"
RESTRICT="binchecks strip test"
IUSE="+cm5 cm4"
REQUIRED_USE="^^ ( cm4 cm5 )"

BDEPEND="
	app-alternatives/bc
	app-alternatives/lex
	app-alternatives/yacc
	dev-lang/perl
	dev-libs/openssl
	sys-apps/kmod
	virtual/libelf
"

# files/ sha256 (kernel/bump.sh): 2de30baeec00b5ca54a7c5e688d18bd83b2f750cb68cd19510bf1e55b57a1ded

pkg_setup() {
	if use cm5; then
		UCONSOLE_PLATFORM=cm5 UCONSOLE_DEFCONFIG=bcm2712_defconfig UCONSOLE_IMAGE=kernel_2712 UCONSOLE_DTBS=bcm2712
	else
		UCONSOLE_PLATFORM=cm4 UCONSOLE_DEFCONFIG=bcm2711_defconfig UCONSOLE_IMAGE=kernel8 UCONSOLE_DTBS=bcm2711-rpi-cm4
	fi
}

src_prepare() {
	eapply "${FILESDIR}"/patches/uconsole-7.3

	# built into the kernel (CONFIG_EXTRA_FIRMWARE_DIR of uconsole.config)
	mkdir firmware-uconsole || die
	cp "${FILESDIR}"/uconsole-splash.tga firmware-uconsole/ || die

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
	local b=${WORKDIR}/build f line opt
	local fragments=( "${FILESDIR}"/uconsole.config "${FILESDIR}/uconsole-${UCONSOLE_PLATFORM}.config" )

	emake -C "${S}" O="${b}" "${UCONSOLE_MAKEARGS[@]}" "${UCONSOLE_DEFCONFIG}"
	"${S}"/scripts/kconfig/merge_config.sh -m -O "${b}" "${b}/.config" "${fragments[@]}" || die
	emake -C "${S}" O="${b}" "${UCONSOLE_MAKEARGS[@]}" olddefconfig

	# olddefconfig drops an option whose dependencies are not met, and
	# says nothing: each line of the fragments must be in the result
	for f in "${fragments[@]}"; do
		while read -r line; do
			case ${line} in
				"# "*) opt=${line#"# "}; opt=${opt%% *}
				       ! grep -q "^${opt}=" "${b}/.config" ;;
				*) grep -qxF "${line}" "${b}/.config" ;;
			esac || die "not in .config: ${line} (${f##*/})"
		done < <(grep -E '^(CONFIG_|# CONFIG_[A-Z0-9_]+ is not set$)' "${f}")
	done
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
