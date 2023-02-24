use gtfs_rt::{TranslatedString, translated_string::Translation};

pub trait Translate {
  fn into_translation(self) -> TranslatedString;
}

impl Translate for String {
  fn into_translation(self) -> TranslatedString {
    TranslatedString {
      translation: vec![
        Translation {
          text: self,
          language: None,
        }
      ]
    }
  }
}
