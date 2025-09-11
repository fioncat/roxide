rox_home() {
    if ret_path=$(ROXIDE_WRAP="true" {{binary}} $@); then
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

rox() {
    action=$1
    case "${action}" in
    home | mirror)
        rox_home "$@"
        ;;

    *)
        ROXIDE_WRAP="true" {{binary}} "$@"
        ;;
    esac
    return $?
}
