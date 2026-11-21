// Drives the public session API against the local XDB test peer.
use xdb_engine::{Error, Session};

fn main() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let port = args
        .next()
        .expect("fixture port")
        .parse()
        .expect("valid port");
    let scenario = args.next().expect("fixture scenario");
    let session = Session::open(port, "mock-iphone", "wifi", false);
    if scenario == "start-error" {
        assert!(matches!(session, Err(Error::Server { code, message })
            if code == "operation_failed" && message == "Video unavailable"));
        return Ok(());
    }
    if scenario == "unsupported-ios" {
        assert!(
            matches!(session, Err(Error::Server { message, .. }) if message.contains("requires iOS"))
        );
        return Ok(());
    }
    let mut session = session?;
    if scenario == "bad-frame" {
        assert!(
            matches!(session.next_frame(0, 100), Err(Error::Client(message))
            if message == "Invalid XDB BGRA frame")
        );
        return Ok(());
    }
    assert_eq!(scenario, "success");
    let frame = session.next_frame(0, 100)?.expect("frame");
    assert_eq!(
        (frame.serial, frame.width, frame.height, frame.row_pitch),
        (1, 4, 6, 16)
    );
    assert_eq!(frame.display["orientation"], "rot90");
    assert_eq!(frame.pixels, [9, 41, 93, 255].repeat(24));
    session.touch(0.25, 0.75, true)?;
    session.touch(0.25, 0.75, false)?;
    session.keyboard(&[4])?;
    session.keyboard(&[])?;
    drop(session);
    Ok(())
}
