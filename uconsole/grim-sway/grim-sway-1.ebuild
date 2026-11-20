# Screenshots in sway: Print a region to the clipboard, Shift+Print the screen to a file.

EAPI=8

DESCRIPTION="Screenshots in sway: Print a region to the clipboard, Shift+Print the screen to a file"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	gui-apps/grim
	gui-apps/slurp
	gui-apps/wl-clipboard
	x11-misc/xdg-user-dirs
	gui-wm/sway
"

src_install() {
	insinto /etc/sway/config.d
	doins "${FILESDIR}"/60-grim.conf
}
