# The hooks of the uConsole for Google Chrome: ECH off by a policy, and a
# second entry of the menu with the flags of WebGPU. The hook is a file
# Portage sources for www-client/google-chrome,
# /etc/portage/env/www-client/google-chrome; it acts when Chrome is
# installed from its .deb, not when a binary package of it is merged.
# Chrome is a PDEPEND: this package is merged first, so the first
# install of Chrome takes the hook.

EAPI=8

DESCRIPTION="The hook of Portage for Google Chrome: ECH off, an entry with WebGPU"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

PDEPEND="www-client/google-chrome"

pkg_pretend() {
	local d
	for d in "${EROOT}"/etc/portage/env "${EROOT}"/etc/portage/env/www-client; do
		if [[ -e ${d} && ! -d ${d} ]]; then
			eerror "${d} is a file; this package puts a file into it."
			eerror "Make it a directory, with the file in it under a name of its own."
			die "${d} is not a directory"
		fi
	done
}

src_install() {
	insinto /etc/portage/env/www-client
	doins "${FILESDIR}"/google-chrome
}

pkg_postinst() {
	if has_version www-client/google-chrome; then
		elog "Google Chrome is installed already, with no hook. Install it again"
		elog "to take the hook:"
		elog
		elog "  emerge -1 www-client/google-chrome"
	fi
}
