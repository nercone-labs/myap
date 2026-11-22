use protowire::{Message, Value, WireError};

#[test]
fn reads_the_examples_of_the_encoding_guide() {
    // protobuf.dev/programming-guides/encoding: field 1 = 150 is 08 96 01.
    let fields: Vec<_> = Message::new(&[0x08, 0x96, 0x01]).collect::<Result<_, _>>().unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].number, 1);
    assert_eq!(fields[0].value, Value::Varint(150));

    // Field 2 = "testing": 12 07 74 65 73 74 69 6e 67.
    let bytes = [0x12, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g'];
    let field = Message::new(&bytes).next().unwrap().unwrap();
    assert_eq!(field.number, 2);
    assert_eq!(field.value.as_str().unwrap(), "testing");

    // Packed repeated field 4 = [3, 270, 86942]: 22 06 03 8E 02 9E A7 05.
    let bytes = [0x22, 0x06, 0x03, 0x8e, 0x02, 0x9e, 0xa7, 0x05];
    let field = Message::new(&bytes).next().unwrap().unwrap();
    let values: Vec<u64> = field.value.packed_varints().collect::<Result<_, _>>().unwrap();
    assert_eq!(values, vec![3, 270, 86942]);
}

#[test]
fn zigzag() {
    // The table of the encoding guide.
    assert_eq!(Message::zigzag(0), 0);
    assert_eq!(Message::zigzag(1), -1);
    assert_eq!(Message::zigzag(2), 1);
    assert_eq!(Message::zigzag(3), -2);
    assert_eq!(Message::zigzag(0x7fff_fffe), 0x3fff_ffff);
    assert_eq!(Message::zigzag(0x7fff_ffff), -0x4000_0000);
    assert_eq!(Message::zigzag(u64::MAX), i64::MIN);
}

#[test]
fn fixed_and_float() {
    let mut bytes = vec![0x09];
    bytes.extend_from_slice(&1.5f64.to_le_bytes());
    bytes.push(0x15);
    bytes.extend_from_slice(&2.25f32.to_le_bytes());
    let fields: Vec<_> = Message::new(&bytes).collect::<Result<_, _>>().unwrap();
    assert_eq!(fields[0].value.as_f64().unwrap(), 1.5);
    assert_eq!(fields[1].value.as_f64().unwrap(), 2.25);
}

#[test]
fn errors() {
    assert_eq!(Message::new(&[0x08]).next(), Some(Err(WireError::Truncated)));
    assert_eq!(Message::new(&[0x12, 0x05, 0x01]).next(), Some(Err(WireError::Truncated)));
    assert_eq!(Message::new(&[0x0b]).next(), Some(Err(WireError::UnsupportedWireType(3))));
    assert_eq!(Message::new(&[0x00, 0x00]).next(), Some(Err(WireError::InvalidFieldNumber)));
    let long = [0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f];
    assert_eq!(Message::new(&long).next(), Some(Err(WireError::VarintTooLong)));
    // After an error, the message ends.
    let mut m = Message::new(&[0x08]);
    m.next();
    assert_eq!(m.next(), None);
    assert_eq!(Value::Varint(1).as_bytes(), Err(WireError::UnexpectedType));
}

#[test]
fn largest_varint() {
    let bytes = [0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
    let field = Message::new(&bytes).next().unwrap().unwrap();
    assert_eq!(field.value, Value::Varint(u64::MAX));
}
