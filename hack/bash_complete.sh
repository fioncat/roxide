_rox() {
	local cmd=${COMP_WORDS[0]}
	local cmp_args=("${COMP_WORDS[@]:1}")
	local items=($($cmd complete "${cmp_args[@]}" 2>/dev/null))

	local flags=${items[0]}
	if [[ $flags -eq "1" ]]; then
		# No space
		compopt -o nospace
	fi
	local items=(${items[@]:1})
	COMPREPLY=($(compgen -W "${items[*]}" -- "${COMP_WORDS[COMP_CWORD]}"))
}

complete -o default -F _rox rox
