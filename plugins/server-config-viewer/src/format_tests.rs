use super::*;

#[test]
fn a_name_of_its_own_decides_in_any_case() {
    for (name, format) in [
        ("nginx.conf", Format::Nginx),
        ("NGINX.CONF", Format::Nginx),
        ("example.com.nginx", Format::Nginx),
        ("httpd.conf", Format::Apache),
        ("apache2.conf", Format::Apache),
        (".htaccess", Format::Apache),
        ("old.htaccess", Format::Apache),
        ("ssh_config", Format::Ssh),
        ("sshd_config", Format::Ssh),
        ("crontab", Format::Crontab),
        ("hosts", Format::Hosts),
        ("Hosts", Format::Hosts),
    ] {
        assert_eq!(Format::by_name(name), Some(format), "{name}");
    }
    for name in [
        "example.conf",
        "hosts.allow",
        "my_crontab",
        "nginx.conf.bak",
        "",
    ] {
        assert_eq!(Format::by_name(name), None, "{name}");
    }
    assert!(is_conf("example.conf") && is_conf("A.CONF"));
    assert!(!is_conf(".conf") && !is_conf("conf") && !is_conf("x.config"));
}

#[test]
fn an_nginx_like_conf_is_taken_by_its_first_lines() {
    for head in [
        "server {\n    listen 80;\n}\n",
        "# comment\n\nhttp {\n",
        "events{\n",
        "upstream app {\n  server 127.0.0.1:3000;\n}\n",
        "user www-data;\nworker_processes auto;\n",
        "\u{feff}  location / {\n",
        "; old\nif ($a = b) {\n",
        "include mime.types;\n",
    ] {
        assert_eq!(sniff(head.as_bytes()), Some(Format::Nginx), "{head}");
    }
}

#[test]
fn an_apache_like_conf_is_taken_by_its_first_lines() {
    for head in [
        "<VirtualHost *:80>\n    ServerName example.com\n</VirtualHost>\n",
        "# Apache\nLoadModule rewrite_module modules/mod_rewrite.so\n",
        "<IfModule mod_ssl.c>\n",
        "ServerRoot \"/etc/httpd\"\n",
        "RewriteEngine On\n",
    ] {
        assert_eq!(sniff(head.as_bytes()), Some(Format::Apache), "{head}");
    }
}

#[test]
fn an_ini_like_conf_and_one_of_no_known_shape_are_declined() {
    for head in [
        "[section]\nkey = value\n",
        "; comment\n[main]\n",
        "key = value\nserver {\n",
        "net.ipv4.ip_forward=1\n",
        "key=value;\n",
        "name: value\n",
        "nameserver 1.1.1.1\nsearch lan\n",
        "/usr/local/lib\n",
        "",
        "\n\n# only comments\n",
    ] {
        assert_eq!(sniff(head.as_bytes()), None, "{head}");
    }
    // The first line that is no comment decides alone.
    assert_eq!(sniff(b"nameserver 1.1.1.1\nserver {\n"), None);
}
