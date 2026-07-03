macro_rules! build_attr_u64 {
    ($field:ident) => {
        paste::paste! {
            pub fn [<with_ $field>](&mut self, $field: u64) -> &mut Self {
                self.$field = Some($field);
                self
            }
        }
    };
}

macro_rules! build_attr_String {
    ($field:ident) => {
        paste::paste! {
            pub fn [<with_ $field>](&mut self, $field: &str) -> &mut Self {
                self.$field = Some($field.to_string());
                self
            }
        }
    };
}

macro_rules! get_attr_u64 {
    ($field:ident) => {
        paste::paste! {
            pub fn $field(&self) -> u64 {
                self.$field
            }
        }
    };
}

macro_rules! get_attr_str {
    ($field:ident) => {
        paste::paste! {
            pub fn $field(&self) -> &str {
                self.$field.as_ref()
            }
        }
    };
}
