mod helpers;

use helpers::{roundtrip, wayc, ways, TestClient, TestServer};

use ways::protocol::{wl_compositor, wl_output};

use wayc::protocol::wl_compositor::WlCompositor as ClientCompositor;
use wayc::protocol::wl_output::WlOutput as ClientOutput;

use std::sync::{Arc, Mutex};

#[test]
fn client_user_data() {
    let mut server = TestServer::new();
    let clients = Arc::new(Mutex::new(Vec::new()));

    struct HasOutput;
    struct HasCompositor;

    server.display.create_global::<wl_output::WlOutput, _>(1, {
        let clients = clients.clone();
        move |newo, _| {
            let output = newo.implement_dummy();
            let client = output.as_ref().client().unwrap();
            let ret = client.data_map().insert_if_missing(|| HasOutput);
            // the data should not be already here
            assert!(ret);
            clients.lock().unwrap().push(client);
        }
    });
    server
        .display
        .create_global::<wl_compositor::WlCompositor, _>(1, {
            let clients = clients.clone();
            move |newo, _| {
                let compositor = newo.implement_dummy();
                let client = compositor.as_ref().client().unwrap();
                let ret = client.data_map().insert_if_missing(|| HasCompositor);
                // the data should not be already here
                assert!(ret);
                clients.lock().unwrap().push(client);
            }
        });

    let mut client = TestClient::new(&server.socket_name);
    let manager = wayc::GlobalManager::new(&client.display);

    roundtrip(&mut client, &mut server).unwrap();

    // Instantiate the globals
    manager
        .instantiate_exact::<ClientOutput, _>(1, |newp| newp.implement_dummy())
        .unwrap();

    roundtrip(&mut client, &mut server).unwrap();

    {
        let clients = clients.lock().unwrap();

        assert!(clients.len() == 1);
        assert!(clients[0].data_map().get::<HasOutput>().is_some());
        assert!(clients[0].data_map().get::<HasCompositor>().is_none());
    }

    manager
        .instantiate_exact::<ClientCompositor, _>(1, |newp| newp.implement_dummy())
        .unwrap();

    roundtrip(&mut client, &mut server).unwrap();

    let clients = clients.lock().unwrap();

    assert!(clients.len() == 2);
    assert!(clients[0].equals(&clients[1]));
    assert!(clients[0].data_map().get::<HasCompositor>().is_some());
    assert!(clients[0].data_map().get::<HasOutput>().is_some());
    assert!(clients[1].data_map().get::<HasCompositor>().is_some());
    assert!(clients[1].data_map().get::<HasOutput>().is_some());
}
