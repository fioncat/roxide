_rox() {
	local cmd=${words[1]}
	local cmp_args=("${words[@]:1}")
	local complete_index=$(expr $CURRENT - 1)
	local items=($(ROXIDE_INIT="zsh" ROXIDE_COMPLETE_INDEX="$complete_index" $cmd "${cmp_args[@]}" 2>/dev/null))

	local flags=${items[1]}
	local items=("${items[@]:1}")
	case "${flags}" in
	"0")
		# Files
		_arguments '*:filename:'"_files"
		;;
	"1")
		# Directories
		_arguments '*:dirname:_files -/'
		;;
	"2")
		# With space
		_describe 'command' items
		;;
	"3")
		# No space
		_describe 'command' items -S ''
		;;
	esac
}

compdef _rox rox
