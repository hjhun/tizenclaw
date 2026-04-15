use crate::error::ApiError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
    pub retry: Option<u64>,
}

#[derive(Debug, Default)]
pub struct SseParser;

impl SseParser {
    pub fn parse(input: &str) -> Result<Vec<SseEvent>, ApiError> {
        let mut events = Vec::new();
        let mut builder = EventBuilder::default();

        for raw_line in input.split('\n') {
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            if line.is_empty() {
                if let Some(event) = builder.finish() {
                    events.push(event);
                }
                continue;
            }

            if let Some(comment) = line.strip_prefix(':') {
                if comment.contains('\0') {
                    return Err(ApiError::SseParse("comment line contained NUL".to_string()));
                }
                continue;
            }

            let (field, value) = match line.split_once(':') {
                Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                None => (line, ""),
            };

            match field {
                "event" => builder.event = Some(value.to_string()),
                "data" => builder.data.push(value.to_string()),
                "id" => builder.id = Some(value.to_string()),
                "retry" => {
                    if let Ok(parsed) = value.parse::<u64>() {
                        builder.retry = Some(parsed);
                    }
                }
                _ => {}
            }
        }

        if let Some(event) = builder.finish() {
            events.push(event);
        }

        Ok(events)
    }
}

#[derive(Debug, Default)]
struct EventBuilder {
    event: Option<String>,
    data: Vec<String>,
    id: Option<String>,
    retry: Option<u64>,
}

impl EventBuilder {
    fn finish(&mut self) -> Option<SseEvent> {
        if self.event.is_none() && self.data.is_empty() && self.id.is_none() && self.retry.is_none()
        {
            return None;
        }

        let event = SseEvent {
            event: self.event.take(),
            data: self.data.join("\n"),
            id: self.id.take(),
            retry: self.retry.take(),
        };
        self.data.clear();
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::{SseEvent, SseParser};

    #[test]
    fn parses_multiline_sse_frames() {
        let input =
            "event: message\nid: 1\ndata: hello\ndata: world\n\n:data-only\n\ndata: tail\n\n";
        let events = SseParser::parse(input).expect("sse parsing should succeed");
        assert_eq!(
            events,
            vec![
                SseEvent {
                    event: Some("message".to_string()),
                    data: "hello\nworld".to_string(),
                    id: Some("1".to_string()),
                    retry: None,
                },
                SseEvent {
                    event: None,
                    data: "tail".to_string(),
                    id: None,
                    retry: None,
                },
            ]
        );
    }
}
