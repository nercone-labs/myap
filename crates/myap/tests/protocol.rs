//! The JSON the Android application writes (the same strings as `ProtocolTest.kt` of the
//! application), read by the core.

use myap::*;
use routing::TravelMode;

#[test]
fn reads_the_inputs_of_the_application() {
    let plan: Input = serde_json::from_str(r#"{"type":"plan","now":1,"origin":{"kind":"my_location"},"destination":{"kind":"place","place":{"id":"b","source":"coordinate","name":"東京タワー","coordinate":{"latitude":35.6586,"longitude":139.7454},"hours":[],"links":[],"filled":[]}},"stops":[],"mode":"car","avoid":{"tolls":false,"highways":false,"ferries":false,"unpaved":false,"narrow":false,"steps":false,"hills":false,"level_crossings":false,"crossing_turns":false,"unlit":false},"router":{"kind":"automatic"}}"#).unwrap();
    let Input::Plan { origin, destination, mode, router, .. } = plan else { panic!() };
    assert_eq!(origin, Endpoint::MyLocation);
    assert!(matches!(destination, Endpoint::Place { place } if place.name == "東京タワー"));
    assert_eq!(mode, TravelMode::Car);
    assert_eq!(router, RouterChoice::Automatic);

    let fetched: Input =
        serde_json::from_str(r#"{"type":"fetched","now":2,"id":3,"response":{"status":200,"body":"x","headers":[["Set-Cookie","a=b"]]}}"#).unwrap();
    let Input::Fetched { response, .. } = fetched else { panic!() };
    assert_eq!(response.headers, vec![("Set-Cookie".to_string(), "a=b".to_string())]);

    let snapshot: Input = serde_json::from_str(r#"{"type":"snapshot","now":4}"#).unwrap();
    assert_eq!(snapshot, Input::Snapshot { now: 4 });

    let only: Input = serde_json::from_str(r#"{"type":"plan","now":5,"origin":{"kind":"my_location"},"destination":{"kind":"my_location"},"stops":[],"mode":"foot","avoid":{"tolls":true,"highways":false,"ferries":false,"unpaved":false,"narrow":false,"steps":false,"hills":false,"level_crossings":false,"crossing_turns":false,"unlit":false},"router":{"kind":"only","router":"myap"}}"#).unwrap();
    assert!(matches!(only, Input::Plan { router: RouterChoice::Only(RouterKind::Myap), .. }));

    let area: Input =
        serde_json::from_str(r#"{"type":"avoid_area","now":8,"name":"工事","center":{"latitude":35.0,"longitude":139.0},"radius_m":150.0}"#).unwrap();
    assert!(matches!(area, Input::AvoidArea { hours: None, .. }));
    assert_eq!(serde_json::from_str::<Input>(r#"{"type":"avoid_ahead","now":9}"#).unwrap(), Input::AvoidAhead { now: 9 });

    let built: Input = serde_json::from_str(r#"{"type":"graph_built","now":6,"id":7,"result":{"Err":"no roads"}}"#).unwrap();
    assert_eq!(built, Input::GraphBuilt { now: 6, id: 7, result: Err("no roads".into()) });
}
