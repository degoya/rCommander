# rCommander shell integration for bash: before every prompt the folder (OSC 7) and the prompt
# marks (OSC 133). rCommander runs it through PROMPT_COMMAND ('eval "$RC_SHELL_INTEGRATION"');
# To add it yourself: save it as ~/.rcommander.bash and put
# PROMPT_COMMAND="source ~/.rcommander.bash${PROMPT_COMMAND:+;$PROMPT_COMMAND}" in ~/.bashrc.
# Started by rCommander, every mark ends in ;rc=$RC_SHELL_NONCE, the session's nonce: a mark a
# program prints carries none and is ignored.
# shellcheck shell=bash disable=SC1003  # the ST of an OSC is ESC and a backslash
__rc_status=$?
if [[ -z ${__rc_loaded-} ]]; then
    __rc_loaded=1
    __rc_mark=${RC_SHELL_NONCE:+;rc=$RC_SHELL_NONCE}
    # The folder as a file URL, every byte but [A-Za-z0-9/._~-] percent-encoded.
    __rc_osc7() {
        local LC_ALL=C path=$PWD encoded='' char i
        for ((i = 0; i < ${#path}; i++)); do
            char=${path:i:1}
            case $char in
                [A-Za-z0-9/._~-]) encoded+=$char ;;
                *)
                    printf -v char '%%%02X' "'$char"
                    encoded+=$char
                    ;;
            esac
        done
        printf '\e]7;file://%s%s%s\e\\' "${HOSTNAME-}" "$encoded" "$__rc_mark"
    }
    __rc_precmd() {
        printf '\e]133;D;%s%s\e\\' "$1" "$__rc_mark"
        __rc_osc7
        printf '\e]133;A%s\e\\' "$__rc_mark"
        # The end of the prompt, once in PS1 whatever ~/.bashrc made of it.
        [[ $PS1 == *'133;B'* ]] || PS1+="\[\e]133;B$__rc_mark\e\\\\\]"
    }
    __rc_return() { return "$1"; }
fi
__rc_precmd "$__rc_status"
__rc_return "$__rc_status"
