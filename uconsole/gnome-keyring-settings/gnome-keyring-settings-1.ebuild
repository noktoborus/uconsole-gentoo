# ssh and git find the agent of the user, gcr-ssh-agent; the password of
# the login unlocks the keyring (pam_gnome_keyring of pambase). Turn on
# for each user: systemctl --global enable gnome-keyring-daemon.socket
# gcr-ssh-agent.socket.

EAPI=8

DESCRIPTION="The agent of SSH of each user is gcr-ssh-agent"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	gnome-base/gnome-keyring[pam]
	app-crypt/gcr:4
	sys-auth/pambase[gnome-keyring]
"

src_install() {
	insinto /etc/profile.d
	newins "${FILESDIR}"/ssh-agent.sh uconsole-ssh-agent.sh
}
