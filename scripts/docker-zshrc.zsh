export EDITOR='nvim'
export SHELL=$(which zsh)

# Init zsh completion
autoload -Uz compinit
compinit

# Init roxide completion
source <(/usr/local/bin/roxide init zsh)
alias zz='rox home'

# The starship shell prompt
eval "$(starship init zsh)"
