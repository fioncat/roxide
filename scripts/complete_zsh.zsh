_roxide() {
	local cmd=${words[1]}
	local cmp_args=("${words[@]:1}")
	local items=($(roxide complete "${cmp_args[@]}"))

	local flags=${items[1]}
	local items=("${items[@]:1}")
	if [[ $flags -eq "1" ]]; then
		# No space
		_describe 'command' items -S ''
	else
		_describe 'command' items
	fi
}

compdef _roxide roxide
