use super::generated::*;

#[test]
fn known_open_enum_value_deserializes_to_known_variant() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"model":"gpt-4o-transcribe"}"#.to_vec(),
    };

    let decoded: GetTranscriptionResponse =
        operations::get_transcription::decode_get_transcription_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        GetTranscriptionResponse::Ok(value) => {
            assert_eq!(value.model, Some(AudioTranscriptionModel::Gpt4oTranscribe));
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn unknown_open_enum_value_deserializes_to_other() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"model":"gpt-custom-transcribe"}"#.to_vec(),
    };

    let decoded: GetTranscriptionResponse =
        operations::get_transcription::decode_get_transcription_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        GetTranscriptionResponse::Ok(value) => {
            assert_eq!(
                value.model,
                Some(AudioTranscriptionModel::Other(
                    "gpt-custom-transcribe".to_owned(),
                ))
            );
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn other_open_enum_value_serializes_as_string() {
    let value: AudioTranscriptionModel = AudioTranscriptionModel::Other("gpt-custom-transcribe".to_owned());
    assert_eq!(value.as_str(), "gpt-custom-transcribe");

    let encoded = serde_json::to_value(value).expect("serialized model");
    assert_eq!(encoded, serde_json::json!("gpt-custom-transcribe"));
}
