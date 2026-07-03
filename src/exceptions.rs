use std::array::TryFromSliceError;
use std::num::{ParseIntError, ParseFloatError};
use std::io;
use std::string::FromUtf8Error;

#[derive(Debug)]
pub enum CustomErr {
    Tokenize(String),
    ParseNumb,
    SyntaxError(String),
    Internal,
    ReadPage(String),
    ParsePage(String),
    ValidateAST(String),
}


impl From<ParseIntError> for CustomErr {
    fn from(_value: ParseIntError) -> Self {
        CustomErr::ParseNumb 
    }
}

impl From<ParseFloatError> for CustomErr {
    fn from(_value: ParseFloatError) -> Self {
        CustomErr::ParseNumb
    }
}

impl From<io::Error> for CustomErr {
    fn from(value: io::Error) -> Self {
        CustomErr::ReadPage(value.to_string())
    }
}

impl From<TryFromSliceError> for CustomErr {
    fn from(_value: TryFromSliceError) -> Self {
        CustomErr::Internal
    }
}

impl From<FromUtf8Error> for CustomErr {
    fn from(_value: FromUtf8Error) -> Self {
        CustomErr::ParsePage("Invalid utf8 sequence".to_string())
    }
}
