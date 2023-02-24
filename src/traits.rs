use gtfs_rt::{translated_string::Translation, TranslatedString};

pub trait Translate {
  fn into_translation(self) -> TranslatedString;
}

impl Translate for String {
  fn into_translation(self) -> TranslatedString {
    TranslatedString {
      translation: vec![Translation {
        text: self,
        language: None,
      }],
    }
  }
}
