_roxide() {
	local cmd=${words[1]}
	local cmp_args=("${words[@]:1}")
	local items=($($cmd complete "${cmp_args[@]}" 2>/dev/null))

	local flags=${items[1]}
	local items=("${items[@]:1}")
	case "${flags}" in
		"0")
			_describe 'command' items
			;;
		"1")
			# No space
			_describe 'command' items -S ''
			;;
		"2")
			# Files
			_arguments '*:filename:'"_files"
	esac
}

compdef _roxide roxide
compdef _roxide _roxide_base
