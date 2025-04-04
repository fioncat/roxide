{{name}}_home() {
	if ret_path=$({{binary}} $@); then
		if [ -z "$ret_path" ]; then
			return
		fi
		if [ -d "$ret_path" ]; then
			cd $ret_path
			return
		fi
		if [ -n "$ret_path" ]; then
			echo $ret_path
		fi
		return
	fi
	return 1
}

{{name}}() {
	action=$1
	case "${action}" in
		home)
			{{name}}_home "$@"
			;;

		*)
			{{binary}} "$@"
			;;
	esac
	return $?
}
