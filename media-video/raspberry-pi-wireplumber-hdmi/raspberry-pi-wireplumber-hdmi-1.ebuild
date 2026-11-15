# The HDMI output of vc4-hdmi after a display is plugged. WirePlumber
# probes a card once, and vc4-hdmi opens no PCM with no display, so a
# display plugged after the start gets no output. A rule of udev on a
# hotplug of DRM starts a oneshot unit in the manager of each user; the
# unit asks WirePlumber by D-Bus (org.freedesktop.ReserveDevice1) to make
# again each vc4hdmi card with a display in and no HDMI profile, so it
# probes it with the display in. notes/hdmi-audio-hotplug.md of the
# repository.

EAPI=8

PYTHON_COMPAT=( python3_{11..14} )

inherit python-single-r1 systemd udev

DESCRIPTION="The HDMI output of vc4-hdmi in WirePlumber after a hotplug"
HOMEPAGE="https://www.clockworkpi.com/uconsole"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="arm64"
REQUIRED_USE="${PYTHON_REQUIRED_USE}"

# amixer (alsa-utils), pw-dump (pipewire), the reservation of the card
# (wireplumber), systemctl, busctl and udev (systemd), stat (coreutils)
RDEPEND="
	${PYTHON_DEPS}
	app-shells/bash
	media-sound/alsa-utils
	media-video/pipewire
	media-video/wireplumber
	sys-apps/coreutils
	sys-apps/systemd
"

src_install() {
	python_scriptinto /usr/libexec
	python_doscript "${FILESDIR}"/${PN}
	exeinto /usr/libexec
	doexe "${FILESDIR}"/${PN}-udev
	systemd_douserunit "${FILESDIR}"/${PN}.service
	udev_dorules "${FILESDIR}"/90-${PN}.rules
}

pkg_postinst() {
	udev_reload
}

pkg_postrm() {
	udev_reload
}
