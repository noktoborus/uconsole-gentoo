# emerge builds C and C++ under ccache: FEATURES=ccache and CCACHE_DIR for
# each package by package.env (*/*), read as a part of make.conf; the
# directory owned by portage (emerge compiles as portage). Removing the
# package turns ccache off.

EAPI=8

DESCRIPTION="C and C++ under ccache for emerge: the settings and the directory"
S=${WORKDIR}

LICENSE="metapackage"
SLOT="0"
KEYWORDS="amd64 arm64 loong ppc64"

RDEPEND="
	acct-user/portage
	dev-util/ccache
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
	newins - portage-ccache.conf <<-EOT
		FEATURES="ccache"
		CCACHE_DIR="/var/cache/ccache"
	EOT
	insinto /etc/portage/package.env
	newins - portage-ccache <<-EOT
		*/* portage-ccache.conf
	EOT
	keepdir /var/cache/ccache
	fowners portage:portage /var/cache/ccache
	fperms 2775 /var/cache/ccache
}

pkg_postinst() {
	elog "emerge builds C and C++ under ccache now; remove this package to stop it."
	elog "One package with no ccache: a line of package.env with FEATURES=\"-ccache\"."
	elog "To see the cache from a shell:"
	elog "  CCACHE_DIR=/var/cache/ccache ccache -s"
}
