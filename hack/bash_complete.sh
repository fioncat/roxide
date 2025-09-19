_rox() {
	local cmd="${COMP_WORDS[0]}"
	local complete_index=$COMP_CWORD
	local items=($(ROXIDE_INIT="bash" ROXIDE_COMPLETE_INDEX="$complete_index" $cmd "${COMP_WORDS[@]:1}" 2>/dev/null))

	if [ ${#items[@]} -eq 0 ]; then
		return
	fi

	local flags="${items[0]}"
	local items=("${items[@]:1}")

	case "${flags}" in
	"0")
		# Files
		COMPREPLY=($(compgen -f -- "${COMP_WORDS[COMP_CWORD]}"))
		;;
	"1")
		# Directories
		COMPREPLY=($(compgen -d -- "${COMP_WORDS[COMP_CWORD]}"))
		;;
	"2")
		# With space
		COMPREPLY=($(compgen -W "${items[*]}" -- "${COMP_WORDS[COMP_CWORD]}"))
		;;
	"3")
		# No space
		COMPREPLY=($(compgen -W "${items[*]}" -- "${COMP_WORDS[COMP_CWORD]}"))
		# Remove trailing space by setting compopt
		compopt -o nospace
		;;
	esac
}

complete -F _rox rox
