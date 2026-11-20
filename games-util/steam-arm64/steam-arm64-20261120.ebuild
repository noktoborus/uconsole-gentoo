# The launcher of the native arm64 Steam client of Valve: /usr/bin/steam
# installs the client into the Steam root of the user on the first run,
# from the manifest of Valve, and runs it. The client keeps itself
# current; this package has no file of Valve. The client needs a kernel of
# 4K pages.

EAPI=8

inherit xdg

DESCRIPTION="Launcher of the native arm64 Steam client of Valve"
HOMEPAGE="https://store.steampowered.com/about/"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="arm64"
IUSE="beta"

# the tray: the GTK 2 appindicator of Valve needs these of the system
RDEPEND="
	app-arch/unzip
	dev-libs/libdbusmenu[gtk]
	net-misc/curl
	sys-apps/coreutils
	x11-libs/gtk+:2
	!games-util/steam
	!games-util/steam-launcher
"

src_install() {
	dobin "${FILESDIR}"/steam
	insinto /etc
	newins - steam-arm64.conf < <(
		sed "s/@CHANNEL@/$(usex beta publicbeta stable)/" "${FILESDIR}"/steam-arm64.conf || die
	)
	insinto /usr/share/applications
	doins "${FILESDIR}"/steam.desktop
}

pkg_postinst() {
	xdg_pkg_postinst
	elog "The first run of steam installs the client of Valve into"
	elog "~/.local/share/Steam: about 1 GB to fetch, 2.5 GB on disk."
	elog "The client is not free software: its terms are the Steam"
	elog "Subscriber Agreement of Valve."
	elog "The client runs on a kernel of 4K pages only."
	elog "The tray takes the GTK 2 appindicator of the arm64 runtime of Valve"
	elog "into ~/.local/share/Steam/steam-arm64/lib on the first run (about 90 MB to fetch)."
	elog "USE beta sets the channel of the first install only; after it, the"
	elog "client keeps its channel (Settings, Interface, Client Beta Participation)."
}
