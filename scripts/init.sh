_roxide_home() {
	if ret_path=$(roxide $@); then
		if [ -z $ret_path ]; then
			return
		fi
		if [ -d $ret_path ]; then
			cd $ret_path
			return
		fi
		if [ -n $ret_path ]; then
			echo $ret_path
		fi
		return
	fi
	return 1
}

_roxide_base() {
	action=$1
	case "${action}" in
		home)
			_roxide_home "$@"
			;;

		*)
			roxide "$@"
			;;
	esac
	return $?
}
