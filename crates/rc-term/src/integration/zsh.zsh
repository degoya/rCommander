# rCommander shell integration for zsh: before every prompt the folder (OSC 7) and the prompt
# marks (OSC 133). rCommander writes this file as .zshenv into a folder of its own and points
# ZDOTDIR there; it hands ZDOTDIR back at once, so zsh then reads your own files as always.
# To add it yourself, put the part inside `if [[ -o interactive ]]` into ~/.zshrc.
# Started by rCommander, every mark ends in ;rc=$RC_SHELL_NONCE, the session's nonce: a mark a
# program prints carries none and is ignored.
ZDOTDIR=${RC_USER_ZDOTDIR:-$HOME}
unset RC_USER_ZDOTDIR
[[ -r $ZDOTDIR/.zshenv ]] && source $ZDOTDIR/.zshenv
if [[ -o interactive ]]; then
    __rc_mark=${RC_SHELL_NONCE:+;rc=$RC_SHELL_NONCE}
    # The folder as a file URL, every byte but [A-Za-z0-9/._~-] percent-encoded.
    __rc_osc7() {
        emulate -L zsh
        local LC_ALL=C encoded= char
        local -i i
        for (( i = 1; i <= ${#PWD}; i++ )); do
            char=${PWD[i]}
            if [[ $char == [A-Za-z0-9/._~-] ]]; then
                encoded+=$char
            else
                printf -v char '%%%02X' "'$char"
                encoded+=$char
            fi
        done
        printf '\e]7;file://%s%s%s\e\\' "$HOST" "$encoded" "$__rc_mark"
    }
    __rc_precmd() {
        local ret=$?
        printf '\e]133;D;%s%s\e\\' $ret "$__rc_mark"
        __rc_osc7
        printf '\e]133;A%s\e\\' "$__rc_mark"
        # The end of the prompt, once in PS1 whatever a theme made of it.
        [[ $PS1 == *'133;B'* ]] || PS1+=$'%{\e]133;B'"$__rc_mark"$'\e\\%}'
    }
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd __rc_precmd
fi
