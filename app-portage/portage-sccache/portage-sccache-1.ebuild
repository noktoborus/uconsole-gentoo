# emerge builds Rust under sccache: RUSTC_WRAPPER and the directories of
# sccache for each package by package.env (*/*), but not for rustc and
# sccache themselves (env/<category>/<package>, read later, wins); the
# sandbox lets emerge write the directories, owned by portage (emerge
# compiles as portage, FEATURES=userpriv). /var/tmp/portage may be a
# tmpfs: systemd-tmpfiles makes its directory on each boot. Removing the
# package turns sccache off.

EAPI=8

inherit tmpfiles

DESCRIPTION="Rust under sccache for emerge: the settings, the directories, the sandbox"
S=${WORKDIR}

LICENSE="metapackage"
SLOT="0"
KEYWORDS="amd64 arm64 loong ppc64"

RDEPEND="
	acct-user/portage
	dev-util/sccache
"

pkg_pretend() {
	local d
	for d in "${EROOT}"/etc/portage/env "${EROOT}"/etc/portage/package.env; do
		if [[ -e ${d} && ! -d ${d} ]]; then
			eerror "${d} is a file; this package puts a file into it."
			eerror "Make it a directory, with the file in it under a name of its own."
			die "${d} is not a directory"
		fi
	done
}

src_install() {
	insinto /etc/portage/env
	newins - portage-sccache.conf <<-EOT
		RUSTC_WRAPPER="sccache"
		SCCACHE_DIR="/var/cache/sccache"
		SCCACHE_SERVER_TMPDIR="/var/tmp/portage/sccache"
		SCCACHE_MAX_FRAME_LENGTH=104857600
	EOT
	insinto /etc/portage/package.env
	newins - portage-sccache <<-EOT
		*/* portage-sccache.conf
	EOT
	# no sccache for the compiler of Rust, nor for sccache itself
	insinto /etc/portage/env/dev-lang
	newins - rust <<-EOT
		RUSTC_WRAPPER=""
	EOT
	insinto /etc/portage/env/dev-util
	newins - sccache <<-EOT
		RUSTC_WRAPPER=""
	EOT

	insinto /etc/sandbox.d
	newins - 40sccache <<-EOT
		SANDBOX_WRITE="/var/cache/sccache/:/var/tmp/portage/sccache/:"
	EOT
	keepdir /var/cache/sccache
	fowners portage:portage /var/cache/sccache
	fperms 2775 /var/cache/sccache
	newtmpfiles - portage-sccache.conf <<-EOT
		d /var/tmp/portage/sccache 0770 portage portage -
	EOT
}

pkg_postinst() {
	tmpfiles_process portage-sccache.conf

	elog "emerge builds Rust under sccache now; remove this package to stop it."
	elog "To see the cache from a shell:"
	elog "  SCCACHE_DIR=/var/cache/sccache sccache --show-stats"
}
