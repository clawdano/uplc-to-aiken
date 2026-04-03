/// Intermediate representation for decompiled UPLC.
///
/// This IR is designed to be progressively refined: the initial lowering from UPLC
/// produces low-level nodes (Lambda, Apply, etc.), and decompilation passes recognize
/// patterns and replace them with higher-level nodes (IfElse, Match, LetBinding, etc.).
#[derive(Debug, Clone)]
pub enum IrNode {
    // === Low-level (direct from UPLC) ===
    /// A variable reference (De Bruijn index)
    Var(usize),

    /// Lambda abstraction. The String is the synthesized parameter name.
    Lambda {
        param_name: String,
        body: Box<IrNode>,
    },

    /// Function application
    Apply {
        function: Box<IrNode>,
        argument: Box<IrNode>,
    },

    /// A constant value
    Constant(IrConstant),

    /// A Plutus builtin function
    Builtin(IrBuiltin),

    /// Force (unwrap a delayed computation)
    Force(Box<IrNode>),

    /// Delay (suspend a computation)
    Delay(Box<IrNode>),

    /// Error / abort
    Error,

    // === Mid-level (recognized patterns) ===
    /// If-then-else expression
    IfElse {
        condition: Box<IrNode>,
        then_branch: Box<IrNode>,
        else_branch: Box<IrNode>,
    },

    /// Let binding: `let name = value in body`
    LetBinding {
        name: String,
        value: Box<IrNode>,
        body: Box<IrNode>,
    },

    /// A known binary operation: `left op right`
    BinOp {
        op: BinOpKind,
        left: Box<IrNode>,
        right: Box<IrNode>,
    },

    /// A known unary operation
    UnaryOp {
        op: UnaryOpKind,
        operand: Box<IrNode>,
    },

    /// Pattern match / when-is expression
    Match {
        subject: Box<IrNode>,
        branches: Vec<MatchBranch>,
    },

    /// Constructor application (e.g., `Some(x)`, `Pair(a, b)`)
    Constr {
        tag: usize,
        type_hint: Option<String>,
        fields: Vec<IrNode>,
    },

    /// Field access on a constructor
    FieldAccess {
        record: Box<IrNode>,
        field_index: usize,
        field_name: Option<String>,
    },

    // === High-level (Aiken-specific patterns) ===
    /// Function definition (named lambda at top level)
    FnDef {
        name: String,
        params: Vec<String>,
        body: Box<IrNode>,
    },

    /// A validator definition
    Validator {
        name: String,
        params: Vec<ValidatorParam>,
        body: Box<IrNode>,
    },

    /// `expect` pattern (Aiken's expect keyword)
    Expect {
        pattern: Box<IrNode>,
        value: Box<IrNode>,
        body: Box<IrNode>,
    },

    /// Trace expression
    Trace {
        message: Box<IrNode>,
        body: Box<IrNode>,
    },

    /// A named function call
    FnCall {
        function_name: String,
        args: Vec<IrNode>,
    },

    /// A list literal
    ListLit(Vec<IrNode>),

    /// A tuple literal
    TupleLit(Vec<IrNode>),

    /// Byte array literal
    ByteArrayLit(Vec<u8>),

    /// String literal
    StringLit(String),

    /// Integer literal
    IntLit(i128),

    /// Boolean literal
    BoolLit(bool),

    /// Unit / Void
    Unit,

    /// A block of sequenced expressions
    Block(Vec<IrNode>),

    /// A comment annotation (for decompiler notes)
    Comment {
        text: String,
        node: Box<IrNode>,
    },
}

#[derive(Debug, Clone)]
pub struct MatchBranch {
    pub pattern: MatchPattern,
    pub body: IrNode,
}

#[derive(Debug, Clone)]
pub enum MatchPattern {
    /// Match a constructor by tag
    Constructor {
        tag: usize,
        type_hint: Option<String>,
        bindings: Vec<String>,
    },
    /// Wildcard / default case
    Wildcard,
}

#[derive(Debug, Clone)]
pub struct ValidatorParam {
    pub name: String,
    pub type_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub enum IrConstant {
    Integer(i128),
    ByteString(Vec<u8>),
    String(String),
    Bool(bool),
    Unit,
    Data(IrData),
    List(Vec<IrConstant>),
    Pair(Box<IrConstant>, Box<IrConstant>),
}

/// Representation of Plutus Data values
#[derive(Debug, Clone)]
pub enum IrData {
    Constr(u64, Vec<IrData>),
    Map(Vec<(IrData, IrData)>),
    List(Vec<IrData>),
    Integer(i128),
    ByteString(Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum IrBuiltin {
    // Integer arithmetic
    AddInteger,
    SubtractInteger,
    MultiplyInteger,
    DivideInteger,
    ModInteger,
    QuotientInteger,
    RemainderInteger,

    // Integer comparison
    EqualsInteger,
    LessThanInteger,
    LessThanEqualsInteger,

    // ByteString operations
    AppendByteString,
    ConsByteString,
    SliceByteString,
    LengthOfByteString,
    IndexByteString,
    EqualsByteString,
    LessThanByteString,
    LessThanEqualsByteString,

    // Cryptographic
    Sha2_256,
    Sha3_256,
    Blake2b_256,
    VerifyEd25519Signature,

    // String operations
    AppendString,
    EqualsString,
    EncodeUtf8,
    DecodeUtf8,

    // Control
    IfThenElse,
    ChooseUnit,
    Trace,

    // Pair operations
    FstPair,
    SndPair,
    MkPairData,

    // List operations
    ChooseList,
    MkCons,
    HeadList,
    TailList,
    NullList,
    MkNilData,
    MkNilPairData,

    // Data operations
    ConstrData,
    MapData,
    ListData,
    IData,
    BData,
    UnConstrData,
    UnMapData,
    UnListData,
    UnIData,
    UnBData,
    EqualsData,
    SerialiseData,

    // Misc
    ChooseData,

    /// Catch-all for builtins we haven't explicitly modeled
    Other(String),
}

#[derive(Debug, Clone, Copy)]
pub enum BinOpKind {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
    Append,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOpKind {
    Negate,
    Not,
    Length,
    Head,
    Tail,
    IsNull,
    Sha256,
    Blake2b256,
    EncodeUtf8,
    DecodeUtf8,
}

impl std::fmt::Display for BinOpKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinOpKind::Add => write!(f, "+"),
            BinOpKind::Sub => write!(f, "-"),
            BinOpKind::Mul => write!(f, "*"),
            BinOpKind::Div => write!(f, "/"),
            BinOpKind::Mod => write!(f, "%"),
            BinOpKind::Eq => write!(f, "=="),
            BinOpKind::Neq => write!(f, "!="),
            BinOpKind::Lt => write!(f, "<"),
            BinOpKind::Lte => write!(f, "<="),
            BinOpKind::Gt => write!(f, ">"),
            BinOpKind::Gte => write!(f, ">="),
            BinOpKind::And => write!(f, "&&"),
            BinOpKind::Or => write!(f, "||"),
            BinOpKind::Append => write!(f, "++"),
        }
    }
}
