use super::{epsv_port, pasv_port};

#[test]
fn epsv_replies_give_their_port() {
    assert_eq!(
        epsv_port("229 Entering Extended Passive Mode (|||50123|)"),
        Some(50123)
    );
    assert_eq!(epsv_port("229 Extended Passive (!!!21000!)"), Some(21000));
    assert_eq!(
        epsv_port("229 Entering Extended Passive Mode (|1|::1|5|)"),
        None
    );
    assert_eq!(
        epsv_port("229 Entering Extended Passive Mode (|||99999|)"),
        None
    );
    assert_eq!(epsv_port("229 no parentheses"), None);
}

#[test]
fn pasv_replies_give_their_port() {
    assert_eq!(
        pasv_port("227 Entering Passive Mode (192,168,1,10,195,149)."),
        Some(195 * 256 + 149)
    );
    assert_eq!(
        pasv_port("227 Entering Passive Mode 10,0,0,1,4,1"),
        Some(1025)
    );
    assert_eq!(pasv_port("227 Entering Passive Mode (10,0,0,1,4)"), None);
    assert_eq!(
        pasv_port("227 Entering Passive Mode (10,0,0,1,4,300)"),
        None
    );
}
