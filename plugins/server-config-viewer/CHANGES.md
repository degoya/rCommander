# Changes

## 0.1.2

Rebuilt against plugin contract 0.6.0; no other change.

## 0.1.1

Built against plugin contract 0.4.0; nothing else changed.

## 0.1.0

The first release: server configuration files coloured like an editor, each in its own grammar -
nginx (`nginx.conf`, `*.nginx`: directives, blocks, `$variables`, location operators, statements
and strings over lines), Apache httpd and `.htaccess` (`httpd.conf`, `apache2.conf`: directives,
sections `<VirtualHost>` ... `</VirtualHost>`, `%{VAR}` and back-references, lines continued by a
backslash), OpenSSH (`ssh_config`, `sshd_config`: keywords, `Host` and `Match` blocks, `%h`
tokens), `crontab` (the five time fields, `@reboot` and the others, variables) and `hosts` (an
address and its names). Strings, numbers with units, addresses, `on`/`off`/`yes`/`no` and
comments in every one. Any other `.conf` is taken when its first line that is no comment looks like nginx or
Apache, and left to the config viewer when it looks like INI. The setting Encoding reads the file as
UTF-8, UTF-16 LE or Windows-1252; `auto` follows a byte-order mark, else takes UTF-8 when the
file is UTF-8, else Windows-1252. Needs rCommander 0.19.0, which claims files by their name.
