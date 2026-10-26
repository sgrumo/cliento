//! Tests Gmail's fetch items in the vendored `imap-codec` patch (decision record 0012) through
//! the public codecs, independently of the session.

use imap_codec::decode::Decoder;
use imap_codec::encode::Encoder;
use imap_codec::{CommandCodec, ResponseCodec};
use imap_types::IntoStatic;
use imap_types::command::{Command, CommandBody};
use imap_types::fetch::{
    GmLabel, MacroOrMessageDataItemNames, MessageDataItem, MessageDataItemName,
};
use imap_types::response::{Data, Response};
use imap_types::sequence::SequenceSet;

const RESPONSE: &[u8] = b"* 7 FETCH (UID 42 X-GM-MSGID 1278455344230334866 \
X-GM-THRID 1278455344230334865 X-GM-LABELS (\\Inbox Work \"Clients/\\\"Acme\\\"\" {13}\r\n\
Receipts 2026))\r\n";

/// Returns a label as text, a system label with its backslash; quoting and escapes are undone.
fn label_text(label: &GmLabel<'_>) -> String {
    match label {
        GmLabel::System(atom) => format!("\\{}", atom.as_ref()),
        GmLabel::Name(name) => String::from_utf8_lossy(name.as_ref()).into_owned(),
    }
}

/// Returns a `FETCH` response's items, or nothing for any other response.
fn fetch_items(response: &Response<'_>) -> Option<Vec<MessageDataItem<'static>>> {
    let Response::Data(Data::Fetch { items, .. }) = response else {
        return None;
    };
    Some(
        items
            .as_ref()
            .iter()
            .cloned()
            .map(MessageDataItem::into_static)
            .collect(),
    )
}

#[test]
fn gmail_items_decode_from_a_response() {
    let (rest, response) = ResponseCodec::default().decode(RESPONSE).unwrap();

    assert!(rest.is_empty(), "the whole response is read");
    let items = fetch_items(&response).unwrap();
    assert!(
        items.contains(&MessageDataItem::XGmMsgId(1_278_455_344_230_334_866)),
        "X-GM-MSGID"
    );
    assert!(
        items.contains(&MessageDataItem::XGmThrId(1_278_455_344_230_334_865)),
        "X-GM-THRID"
    );
    let Some(MessageDataItem::XGmLabels(labels)) = items
        .iter()
        .find(|item| matches!(item, MessageDataItem::XGmLabels(_)))
    else {
        panic!("X-GM-LABELS in {items:?}");
    };
    assert_eq!(
        labels.iter().map(label_text).collect::<Vec<_>>(),
        ["\\Inbox", "Work", "Clients/\"Acme\"", "Receipts 2026"],
        "a system label, an atom, a quoted string and a literal"
    );
}

#[test]
fn gmail_items_survive_an_encode_and_decode() {
    let codec = ResponseCodec::default();
    let (_, response) = codec.decode(RESPONSE).unwrap();

    let encoded = codec.encode(&response).dump();
    let (rest, again) = codec.decode(&encoded).unwrap();

    assert!(rest.is_empty(), "the whole response is read");
    assert_eq!(fetch_items(&again), fetch_items(&response), "same items");
}

#[test]
fn gmail_item_names_encode_in_a_command() {
    let command = Command::new(
        "A1",
        CommandBody::Fetch {
            sequence_set: SequenceSet::try_from("1:3").unwrap(),
            macro_or_item_names: MacroOrMessageDataItemNames::MessageDataItemNames(vec![
                MessageDataItemName::Uid,
                MessageDataItemName::XGmMsgId,
                MessageDataItemName::XGmThrId,
                MessageDataItemName::XGmLabels,
            ]),
            uid: true,
            modifiers: Vec::new(),
        },
    )
    .unwrap();

    let encoded = CommandCodec::default().encode(&command).dump();
    let (rest, decoded) = CommandCodec::default().decode(&encoded).unwrap();

    assert_eq!(
        encoded, b"A1 UID FETCH 1:3 (UID X-GM-MSGID X-GM-THRID X-GM-LABELS)\r\n",
        "item names"
    );
    assert!(rest.is_empty(), "the whole command is read");
    assert_eq!(decoded, command, "the names decode back");
}

#[test]
fn an_unknown_item_still_fails_to_decode() {
    let line = b"* 3 FETCH (UID 103 THIS-IS-NOT-AN-ITEM 1)\r\n";

    assert!(
        ResponseCodec::default().decode(line).is_err(),
        "only the three Gmail items were added"
    );
}
