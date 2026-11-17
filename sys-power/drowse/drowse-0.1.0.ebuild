# drowse: while every session of logind is idle, drowsed does the actions
# of its stages (a command and its undo, an action of power); its state
# is on the system bus. drowse-inhibit holds the system by hand,
# drowse-status shows the state, for waybar too. With USE=waybar the
# module files go to /etc/xdg/waybar. With USE=sway a file sets the idle
# hint in Sway. /etc/drowse.yaml has the settings.
# The preset enables the service, its config watcher and its start after
# sleep.
#
# The source is a copy of drowse in files/drowse, with the sum below;
# CRATES is its Cargo.lock. The modes of files/ are not kept.
# /etc/drowse.yaml holds the YAML chains; /etc is
# in CONFIG_PROTECT, so a change made on the device is kept on an update.
#
# The profiles of TuneD cm5-powersave, the power save of the first stage,
# and cm5-powersave-no-radio, a deeper step with every radio off, are
# examples in the documentation. With USE=cm5-tuned they are also
# profiles of TuneD in /usr/lib/tuned/profiles, and TuneD comes with
# them.

EAPI=8

CRATES="
	aho-corasick@1.1.5
	annotate-snippets@0.12.18
	anstyle@1.0.14
	arc-swap@1.9.5
	arraydeque@0.5.1
	async-trait@0.1.92
	autocfg@1.5.1
	base62@2.2.6
	base64@0.23.1
	bitflags@1.3.2
	bitflags@2.13.2
	bstr@1.13.1
	build-env@0.3.1
	bumpalo@3.20.4
	bytes@1.12.1
	cfg-if@1.0.5
	core_detect@1.0.0
	crossbeam-deque@0.8.8
	crossbeam-epoch@0.9.21
	crossbeam-utils@0.8.23
	either@1.19.0
	encoding_rs@0.8.44
	encoding_rs_io@0.1.8
	endi@1.1.1
	equivalent@1.0.3
	errno@0.3.15
	fastrand@2.5.0
	foldhash@0.2.0
	futures-core@0.3.34
	futures-io@0.3.34
	futures-task@0.3.34
	futures-util@0.3.34
	getrandom@0.4.3
	glob@0.3.4
	globset@0.4.21
	globwalk@0.8.1
	granit-parser@1.3.0
	hashbrown@0.17.1
	hex@0.4.3
	ignore@0.4.34
	indexmap@2.14.2
	itertools@0.11.0
	itoa@1.0.19
	js-sys@0.3.107
	libc@0.2.191
	libsystemd-sys@0.10.0
	linux-raw-sys@0.12.1
	log@0.4.35
	memchr@2.8.3
	memoffset@0.9.1
	mio@1.2.5
	multiversion_no_op@1.0.0
	nohash-hasher@0.2.0
	normpath@1.5.2
	num-traits@0.2.19
	once_cell@1.21.4
	ordered-stream@0.2.0
	pin-project-lite@0.2.17
	pkg-config@0.3.34
	proc-macro2@1.0.107
	quote@1.0.47
	r-efi@6.0.0
	regex-automata@0.4.18
	regex-syntax@0.8.11
	regex@1.13.1
	rust-i18n-macro@4.2.4
	rust-i18n-support@4.2.4
	rust-i18n@4.2.4
	rustix@1.1.6
	rustversion@1.0.23
	ryu@1.0.23
	same-file@1.0.6
	scopeguard@1.2.0
	serde-saphyr@1.3.0
	serde@1.0.229
	serde_core@1.0.229
	serde_derive@1.0.229
	serde_json@1.0.154
	serde_repr@0.1.21
	serde_spanned@0.6.9
	serde_yaml_ng@0.10.0
	shlex@2.0.1
	signal-hook-registry@1.4.8
	simdutf8@0.1.5
	siphasher@1.0.4
	slab@0.4.12
	smallvec@1.16.5
	socket2@0.6.5
	stable_deref_trait@1.2.1
	syn@2.0.119
	syn@3.0.10
	sys-locale@0.3.2
	tempfile@3.27.0
	tokio-macros@2.7.2
	tokio@1.54.1
	toml@0.8.23
	toml_datetime@0.6.11
	toml_edit@0.22.27
	toml_write@0.1.2
	triomphe@0.1.17
	uds_windows@1.2.1
	unicode-ident@1.0.27
	unicode-width@0.2.3
	unsafe-libyaml@0.2.11
	uuid@1.29.2
	walkdir@2.5.0
	wasi@0.11.1+wasi-snapshot-preview1
	wasm-bindgen-macro-support@0.2.130
	wasm-bindgen-macro@0.2.130
	wasm-bindgen-shared@0.2.130
	wasm-bindgen@0.2.130
	winapi-util@0.1.11
	windows-link@0.2.1
	windows-sys@0.61.2
	winnow@0.7.15
	winnow@1.0.5
	zbus@6.3.0
	zbus_macros@6.3.0
	zbus_utils@6.1.0
	zcheapstr@1.2.0
	zmij@1.0.23
	zruntime@0.2.3
"

inherit cargo systemd

DESCRIPTION="Does the actions of its stages while every session of logind is idle"
HOMEPAGE="https://www.clockworkpi.com/uconsole"
SRC_URI="${CARGO_CRATE_URIS}"
S=${WORKDIR}/drowse

LICENSE="MIT"
# the crates
LICENSE+=" BSD LGPL-2.1+ MIT Unicode-3.0 ZLIB"
SLOT="0"
KEYWORDS="arm64"
IUSE="cm5-tuned sway waybar"

DEPEND="sys-apps/systemd"
RDEPEND="
	${DEPEND}
	sys-apps/dbus
	cm5-tuned? ( sys-apps/tuned )
	sway? (
		gui-wm/sway
		gui-apps/swayidle
	)
	waybar? ( gui-apps/waybar )
"
BDEPEND="virtual/pkgconfig"

# files/ sha256: 2ebbcd1ca4d71bc3f80042ad6bc6f88c2d3ed26a94950137096fc0d1087476fb

src_unpack() {
	cargo_src_unpack
	cp -r --no-preserve=mode "${FILESDIR}"/drowse/. "${S}" || die
}

src_install() {
	cargo_src_install
	systemd_dounit systemd/drowsed.service systemd/drowsed-stage@.service \
		systemd/drowsed-resume.service systemd/drowsed-config.path \
		systemd/drowsed-config.service
	insinto "$(systemd_get_systempresetdir)"
	doins preset/50-drowse.preset
	insinto /usr/share/dbus-1/system.d
	doins dbus/io.github.noktoborus.Drowse.conf
	insinto /etc
	doins etc/drowse.yaml
	# The file as the package gives it, to compare with or to take again:
	# /etc/drowse.yaml is changed by the user.
	insinto /usr/share/drowse
	doins etc/drowse.yaml
	if use waybar; then
		insinto /etc/xdg/waybar
		doins waybar/drowse.jsonc waybar/drowse.css
	fi
	if use sway; then
		insinto /etc/sway/config.d
		doins "${FILESDIR}"/80-idlehint.conf
	fi
	dodoc README.md
	# The profile of TuneD is an example the user copies by hand; kept
	# whole, not compressed.
	docinto examples
	dodoc -r examples/.
	docompress -x "/usr/share/doc/${PF}/examples"
	# TuneD runs the scripts of the profiles by their paths.
	fperms +x "/usr/share/doc/${PF}/examples/tuned/cm5-powersave/usb-autosuspend.sh" \
		"/usr/share/doc/${PF}/examples/tuned/cm5-powersave-no-radio/rfkill-off.sh"
	if use cm5-tuned; then
		insinto /usr/lib/tuned/profiles/cm5-powersave
		doins examples/tuned/cm5-powersave/tuned.conf
		exeinto /usr/lib/tuned/profiles/cm5-powersave
		doexe examples/tuned/cm5-powersave/usb-autosuspend.sh
		insinto /usr/lib/tuned/profiles/cm5-powersave-no-radio
		doins examples/tuned/cm5-powersave-no-radio/tuned.conf
		exeinto /usr/lib/tuned/profiles/cm5-powersave-no-radio
		doexe examples/tuned/cm5-powersave-no-radio/rfkill-off.sh
	fi
}
