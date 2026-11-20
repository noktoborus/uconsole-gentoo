# The UCM of the sound card uConsole for ALSA (alsa-ucm-conf): the
# speakers, or the headphones when the jack is in.

EAPI=8

DESCRIPTION="The UCM of the sound card of the uConsole: speakers or headphones"
HOMEPAGE="https://www.clockworkpi.com/uconsole"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="arm64"

RDEPEND="media-libs/alsa-ucm-conf"

src_install() {
	insinto /usr/share/alsa/ucm2/uConsole
	doins "${FILESDIR}"/uConsole.conf "${FILESDIR}"/HiFi.conf
	dosym ../../uConsole/uConsole.conf /usr/share/alsa/ucm2/conf.d/uConsole/uConsole.conf
}
