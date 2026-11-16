# The kernel of the ClockworkPi uConsole: Raspberry Pi rpi-7.3.y with the
# series of files/patches/uconsole-7.3, for each platform of USE (cm5,
# cm4, both side by side; 16k: the CM5 with 16K pages). Built in the
# builder as each other package; the uConsole takes its binary package.
# The files of the firmware go to /boot, the FAT partition, as
# sys-boot/uconsole-pi-firmware puts its files.
#
# It needs nothing but this directory: the source is the tarball of one
# commit of Raspberry Pi, files/ has the series and the fragments of the
# configuration. kernel/bump.sh gives
# it a new version when files/ or the commit changes.

EAPI=8

inherit mount-boot toolchain-funcs

DESCRIPTION="Kernel of the ClockworkPi uConsole: Raspberry Pi rpi-7.3.y and its patches"
HOMEPAGE="https://github.com/raspberrypi/linux"

UCONSOLE_BASE=e4d5c8fdad5e5a5bffe85bb1a755feb1a65eed1b
SRC_URI="https://github.com/raspberrypi/linux/archive/${UCONSOLE_BASE}.tar.gz -> raspberrypi-linux-${UCONSOLE_BASE}.tar.gz"
S=${WORKDIR}/linux-${UCONSOLE_BASE}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="arm64"
RESTRICT="binchecks strip test"
IUSE="cm5 cm4 16k"
REQUIRED_USE="|| ( cm4 cm5 ) 16k? ( cm5 )"

BDEPEND="
	app-alternatives/bc
	app-alternatives/lex
	app-alternatives/yacc
	dev-lang/perl
	dev-libs/openssl
	sys-apps/kmod
	virtual/libelf
"
# config.txt and cmdline.txt (CONFIG_PROTECT), and the files of the
# firmware of the CM4 (start4.elf, fixup4.dat); the CM5 has them in its
# EEPROM
RDEPEND="sys-boot/uconsole-pi-firmware[cm4?,cm5?]"

# files/ sha256 (kernel/bump.sh): 34ba40901f181379eb5a11d5fb3d9c322cead922b8cabea84b8ffb97654c8b7b

# The kernels to build, one line each: <platform>, its defconfig, the name
# of its kernel file (the name the firmware looks for: kernel_2712.img on
# the CM5, kernel8.img on the CM4), its device trees, the CPU it is tuned
# for (KCFLAGS). The fragment of the platform is
# files/uconsole-<platform>.config.
pkg_setup() {
	UCONSOLE_KERNELS=()
	use cm4 && UCONSOLE_KERNELS+=( "cm4 bcm2711_defconfig kernel8 bcm2711-rpi-cm4 cortex-a72" )
	use cm5 && UCONSOLE_KERNELS+=( "cm5$(usev 16k -16k) bcm2712_defconfig kernel_2712 bcm2712 cortex-a76" )
}

src_prepare() {
	eapply "${FILESDIR}"/patches/uconsole-7.3
	eapply_user
}

src_configure() {
	# The kernel takes no flags of make.conf or of the environment: its
	# own flags, -mtune of the platform (src_compile), and the compiler of
	# the toolchain (ccache and distcc through the path). -mtune, not
	# -mcpu: the kernel picks the instructions of the CPU at boot
	# (alternatives), and -mcpu would fight its -march.
	unset KBUILD_OUTPUT CFLAGS CXXFLAGS CPPFLAGS LDFLAGS ASFLAGS RUSTFLAGS \
		KCFLAGS KCPPFLAGS KAFLAGS KRUSTFLAGS
	UCONSOLE_MAKEARGS=(
		ARCH=arm64
		CC="$(tc-getCC)" HOSTCC="$(tc-getBUILD_CC)"
		LD="$(tc-getLD)" AR="$(tc-getAR)" NM="$(tc-getNM)"
		OBJCOPY="$(tc-getOBJCOPY)" STRIP="$(tc-getSTRIP)"
	)
	local k platform defconfig image dtbs cpu b f line opt fragments
	for k in "${UCONSOLE_KERNELS[@]}"; do
		read -r platform defconfig image dtbs cpu <<< "${k}"
		b=${WORKDIR}/build-${platform}
		fragments=( "${FILESDIR}"/uconsole.config "${FILESDIR}/uconsole-${platform}.config" )

		emake -C "${S}" O="${b}" "${UCONSOLE_MAKEARGS[@]}" "${defconfig}"
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
	done
}

src_compile() {
	local k platform defconfig image dtbs cpu
	for k in "${UCONSOLE_KERNELS[@]}"; do
		read -r platform defconfig image dtbs cpu <<< "${k}"
		emake -C "${S}" O="${WORKDIR}/build-${platform}" "${UCONSOLE_MAKEARGS[@]}" \
			KCFLAGS="-mtune=${cpu}" Image modules dtbs
	done
}

src_install() {
	local k platform defconfig image dtbs cpu b kv kdir
	for k in "${UCONSOLE_KERNELS[@]}"; do
		read -r platform defconfig image dtbs cpu <<< "${k}"
		b=${WORKDIR}/build-${platform}
		kv=$(<"${b}/include/config/kernel.release")
		kdir=/usr/src/linux-${kv}

		# /boot: the kernel under the name the firmware looks for, the
		# device trees of the platform, the overlays (the same for each
		# platform: one tree)
		insinto /boot
		newins "${b}/arch/arm64/boot/Image" "${image}.img"
		doins "${b}"/arch/arm64/boot/dts/broadcom/${dtbs}*.dtb
		insinto /boot/overlays
		doins "${b}"/arch/arm64/boot/dts/overlays/*.dtbo "${S}/arch/arm/boot/dts/overlays/README"

		emake -C "${S}" O="${b}" "${UCONSOLE_MAKEARGS[@]}" \
			INSTALL_MOD_PATH="${ED}" INSTALL_MOD_STRIP=1 modules_install
		rm -f "${ED}/lib/modules/${kv}"/{build,source} || die

		# The tree for external modules (linux-mod-r1): what the kernel
		# itself gives for that, as kernel.spec of the kernel does
		emake -C "${S}" O="${b}" "${UCONSOLE_MAKEARGS[@]}" run-command \
			KBUILD_RUN_COMMAND="${S}/scripts/package/install-extmod-build ${ED}${kdir}"
		insinto "${kdir}"
		doins "${b}/.config" "${b}/System.map"
		dosym "../../..${kdir}" "/lib/modules/${kv}/build"
		# /usr/src/linux: the tree the packages of modules build against.
		# The last kernel of UCONSOLE_KERNELS takes it, the CM5 when it
		# is there: modules of packages are built for that kernel only.
		dosym "linux-${kv}" /usr/src/linux
	done

	# /dev/ntsync has no alias: systemd loads the module at boot
	insinto /usr/lib/modules-load.d
	newins "${FILESDIR}"/modules-load-ntsync.conf ntsync.conf
}
