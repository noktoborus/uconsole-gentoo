# /etc/profile.d/uconsole-ssh-agent.sh: ssh and git find the agent of the
# user, gcr-ssh-agent (gcr-ssh-agent.socket), unless one is set (a
# forwarded agent of ssh). greetd sources /etc/profile before the session
# (source_profile of greetd(5)), so sway and its programs have it too.
if [ -z "${SSH_AUTH_SOCK}" ] && [ -n "${XDG_RUNTIME_DIR}" ]; then
	export SSH_AUTH_SOCK="${XDG_RUNTIME_DIR}/gcr/ssh"
fi
