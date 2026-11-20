# DNS by systemd-resolved: it resolves the names of .local and does not
# answer; avahi announces the uConsole. pkg_postinst points
# /etc/resolv.conf to resolved when there is none.

EAPI=8

inherit systemd

DESCRIPTION="DNS by systemd-resolved; it resolves .local, avahi answers"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	sys-apps/systemd
	net-dns/avahi
"

src_install() {
	insinto "$(systemd_get_utildir)"/resolved.conf.d
	newins "${FILESDIR}"/resolved.conf 50-uconsole.conf
}

pkg_postinst() {
	local stub=/run/systemd/resolve/stub-resolv.conf
	if [[ ! -e ${EROOT}/etc/resolv.conf && ! -L ${EROOT}/etc/resolv.conf ]]; then
		ln -s "${stub}" "${EROOT}"/etc/resolv.conf || die
	elif [[ $(readlink "${EROOT}"/etc/resolv.conf) != "${stub}" ]]; then
		elog "/etc/resolv.conf is not the one of systemd-resolved. To take it:"
		elog "  systemctl enable --now systemd-resolved.service"
		elog "  ln -snf ${stub} /etc/resolv.conf"
	fi
}
