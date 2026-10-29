# rCommander shell integration for PowerShell 7 and Windows PowerShell 5.1: at every prompt the
# folder (OSC 9;9) and the prompt marks (OSC 133), around the prompt you already have.
# rCommander runs it with -EncodedCommand after your profile; to add it yourself, put it at the
# end of your $PROFILE.
if (-not $global:__rcPromptWrapped) {
    $global:__rcPromptWrapped = $true
    $global:__rcPrompt = $function:prompt
    function global:prompt {
        $esc = [char]27
        $out = "$esc]133;D$esc\"
        $location = $executionContext.SessionState.Path.CurrentLocation
        if ($location.Provider.Name -eq 'FileSystem') {
            $out += "$esc]9;9;`"$($location.ProviderPath)`"$esc\"
        }
        $out += "$esc]133;A$esc\"
        if ($global:__rcPrompt) {
            $out += & $global:__rcPrompt
        } else {
            $out += "PS $location> "
        }
        $out + "$esc]133;B$esc\"
    }
}
