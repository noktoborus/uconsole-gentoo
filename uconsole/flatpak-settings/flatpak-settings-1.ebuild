# Flathub as a remote of Flatpak for the system; nothing is installed from
# it. files/flathub.flatpakrepo: https://dl.flathub.org/repo/flathub.flatpakrepo
# as it came on 2026-10-29, sha256
# 3371dd250e61d9e1633630073fefda153cd4426f72f4afa0c3373ae2e8fea03a.

EAPI=8

DESCRIPTION="Flathub as a remote of Flatpak for the system"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	sys-apps/flatpak
"

src_install() {
	# flatpak-remote(5): a remote of the system, preconfigured
	insinto /usr/share/flatpak/remotes.d
	doins "${FILESDIR}"/flathub.flatpakrepo
}
