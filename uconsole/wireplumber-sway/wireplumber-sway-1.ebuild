# The keys of the volume of PipeWire in sway.

EAPI=8

DESCRIPTION="The keys of the volume of PipeWire in sway"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	media-video/wireplumber
	gui-wm/sway
"

src_install() {
	insinto /etc/sway/config.d
	doins "${FILESDIR}"/60-wireplumber.conf
}
