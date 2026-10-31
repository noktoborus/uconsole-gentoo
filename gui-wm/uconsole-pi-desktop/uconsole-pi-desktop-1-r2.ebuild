# The desktop of the ClockworkPi uConsole: sway for each user (the panel
# turned, the session for systemd, idle and lock, the keys of the
# backlight), the login by greetd and gtkgreet (its password unlocks the
# keyring of gnome-keyring, with gcr-ssh-agent the agent of SSH), the
# presets of the services it needs, DNS by
# systemd-resolved (pkg_postinst points /etc/resolv.conf to it), and
# Flathub as a remote of Flatpak for the system (nothing is installed from
# it). Its
# RDEPEND are the programs of the desktop: the set @uconsole takes this
# package only.
#
# files/flathub.flatpakrepo: https://dl.flathub.org/repo/flathub.flatpakrepo
# as it came on 2026-10-29, sha256
# 3371dd250e61d9e1633630073fefda153cd4426f72f4afa0c3373ae2e8fea03a.

EAPI=8

inherit pam systemd

DESCRIPTION="The desktop of the ClockworkPi uConsole: sway, greetd, the presets of its services"
HOMEPAGE="https://www.clockworkpi.com/uconsole"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="~arm64"

# sway and its tools; files: Thunar, gvfs for phones (MTP), cameras, the
# network, the trash, udisks2 mounts by polkit, thunar-volman what is
# plugged in, tumbler and ffmpegthumbnailer the thumbnails, the agent of
# polkit asks the password; Flatpak and the portals; Bluetooth; the sound
# by PipeWire; the login; systemd-homed, its LUKS homes have btrfs; the
# network by NetworkManager; mDNS and DNS-SD by avahi (USE zeroconf of
# CUPS, gvfs, PipeWire take it); the keyring and the agent of SSH.
RDEPEND="
	net-misc/networkmanager
	net-dns/avahi
	gui-wm/sway
	gui-apps/waybar
	gui-apps/swayidle
	gui-apps/swaylock
	gui-apps/foot
	xfce-base/thunar
	xfce-base/thunar-volman
	xfce-base/tumbler
	gnome-base/gvfs
	sys-fs/udisks
	media-video/ffmpegthumbnailer
	gnome-extra/polkit-gnome
	sys-apps/flatpak
	sys-apps/xdg-desktop-portal
	sys-apps/xdg-desktop-portal-gtk
	gui-libs/xdg-desktop-portal-wlr
	net-wireless/blueman
	media-video/pipewire
	media-video/wireplumber
	gui-libs/greetd
	gui-apps/gtkgreet
	sys-apps/systemd[homed]
	sys-auth/pambase[homed]
	sys-fs/btrfs-progs
	gnome-base/gnome-keyring[pam]
	app-crypt/gcr:4
"

src_install() {
	insinto /etc/sway/config.d
	newins "${FILESDIR}"/sway-output.conf 40-uconsole-output.conf
	newins "${FILESDIR}"/sway-uconsole.conf 50-uconsole.conf
	systemd_douserunit "${FILESDIR}"/sway-session.target
	dobin "${FILESDIR}"/backlight

	insinto /etc/greetd
	newins "${FILESDIR}"/greetd-uconsole.toml uconsole.toml
	newins "${FILESDIR}"/greetd-sway-config sway-config
	newins "${FILESDIR}"/greetd-environments environments
	systemd_install_serviced "${FILESDIR}"/greetd-service.conf greetd.service
	newpamd "${FILESDIR}"/greetd-pam uconsole-greetd

	insinto /etc/profile.d
	newins "${FILESDIR}"/ssh-agent.sh uconsole-ssh-agent.sh

	# flatpak-remote(5): a remote of the system, preconfigured; the same
	# name in /etc/flatpak/remotes.d/ takes precedence
	insinto /usr/share/flatpak/remotes.d
	doins "${FILESDIR}"/flathub.flatpakrepo

	insinto "$(systemd_get_utildir)"/resolved.conf.d
	newins "${FILESDIR}"/resolved.conf 50-uconsole.conf
	insinto /usr/lib/NetworkManager/conf.d
	newins "${FILESDIR}"/networkmanager.conf 50-uconsole.conf

	insinto "$(systemd_get_systempresetdir)"
	newins "${FILESDIR}"/system.preset 50-${PN}.preset
	insinto "$(systemd_get_utildir)"/user-preset
	newins "${FILESDIR}"/user.preset 50-${PN}.preset
}

pkg_postinst() {
	# DNS by systemd-resolved: NetworkManager sees the link and gives the
	# servers to resolved (dns=systemd-resolved by default then). An
	# /etc/resolv.conf that is there stays: with no running resolved the
	# link would leave the system with no DNS.
	local stub=/run/systemd/resolve/stub-resolv.conf
	if [[ ! -e ${EROOT}/etc/resolv.conf && ! -L ${EROOT}/etc/resolv.conf ]]; then
		ln -s "${stub}" "${EROOT}"/etc/resolv.conf || die
	elif [[ $(readlink "${EROOT}"/etc/resolv.conf) != "${stub}" ]]; then
		elog "/etc/resolv.conf is not the one of systemd-resolved. To take it:"
		elog
		elog "  systemctl enable --now systemd-resolved.service"
		elog "  ln -snf ${stub} /etc/resolv.conf"
		elog "  systemctl restart NetworkManager.service"
		elog
	fi

	if [[ -z ${REPLACING_VERSIONS} ]]; then
		elog "Turn on the services of the desktop and the login screen:"
		elog
		elog "  systemctl preset NetworkManager.service systemd-resolved.service \\"
		elog "      avahi-daemon.service \\"
		elog "      systemd-networkd.service \\"
		elog "      systemd-networkd-wait-online.service greetd.service \\"
		elog "      systemd-homed.service bluetooth.service"
		elog "  systemctl --global preset pipewire.socket pipewire-pulse.socket wireplumber.service \\"
		elog "      gnome-keyring-daemon.socket gcr-ssh-agent.socket"
		elog "  systemctl set-default graphical.target"
	fi

	local v
	for v in ${REPLACING_VERSIONS}; do
		ver_test "${v}" -lt 1-r2 || continue
		elog "The keyring and the agent of SSH of each user:"
		elog
		elog "  systemctl --global preset gnome-keyring-daemon.socket gcr-ssh-agent.socket"
		elog
		elog "root no longer logs in at the boot: greetd asks for the user."
		break
	done
}
