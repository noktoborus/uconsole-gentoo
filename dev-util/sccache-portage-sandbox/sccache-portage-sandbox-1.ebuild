# sccache under emerge: the sandbox lets it write its directories, and
# systemd-tmpfiles makes them on each boot, owned by portage (emerge runs
# the compiler as portage, FEATURES=userpriv). /var/tmp/portage may be a
# tmpfs. The variables of make.conf stay with the user: pkg_postinst
# names them.

EAPI=8

inherit tmpfiles

DESCRIPTION="Directories and the sandbox rule of sccache for emerge"
S=${WORKDIR}

LICENSE="metapackage"
SLOT="0"
KEYWORDS="amd64 arm64 loong ppc64"

RDEPEND="
	acct-user/portage
	dev-util/sccache
"

src_install() {
	insinto /etc/sandbox.d
	newins - 40sccache <<-EOT
		SANDBOX_WRITE="/var/cache/sccache/:/var/tmp/portage/sccache/:"
	EOT
	newtmpfiles - sccache-portage.conf <<-EOT
		d /var/cache/sccache 2775 portage portage -
		d /var/tmp/portage/sccache 0770 portage portage -
	EOT
}

pkg_postinst() {
	tmpfiles_process sccache-portage.conf

	elog "emerge uses sccache for Rust only when make.conf says so."
	elog "Add to a file of /etc/portage/make.conf/:"
	elog
	elog "  RUSTC_WRAPPER=\"sccache\""
	elog "  SCCACHE_DIR=\"/var/cache/sccache\""
	elog "  SCCACHE_SERVER_TMPDIR=\"/var/tmp/portage/sccache\""
	elog "  SCCACHE_MAX_FRAME_LENGTH=104857600"
	elog
	elog "Packages that must not run under sccache (dev-lang/rust,"
	elog "dev-util/sccache) take an env file, /etc/portage/env/no-sccache:"
	elog
	elog "  RUSTC_WRAPPER=\"\""
	elog
	elog "and lines in /etc/portage/package.env:"
	elog
	elog "  dev-lang/rust no-sccache"
	elog "  dev-util/sccache no-sccache"
	elog
	elog "To see the cache from a shell, set SCCACHE_DIR there too:"
	elog "  SCCACHE_DIR=/var/cache/sccache sccache --show-stats"
}
