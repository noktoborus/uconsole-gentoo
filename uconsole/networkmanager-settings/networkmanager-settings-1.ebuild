# Each connection of NetworkManager asks systemd-resolved to resolve
# .local on its link (connection.mdns=1).

EAPI=8

DESCRIPTION="NetworkManager asks systemd-resolved to resolve .local"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	net-misc/networkmanager
"

src_install() {
	insinto /usr/lib/NetworkManager/conf.d
	newins "${FILESDIR}"/networkmanager.conf 50-uconsole.conf
}
