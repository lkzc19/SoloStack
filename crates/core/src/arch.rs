/// 架构类型。
///
/// MVP 聚焦 aarch64（Apple Silicon），x86_64 作为扩展点预留。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Aarch64,
    X8664,
}

impl Arch {
    /// 当前编译目标平台的架构。
    ///
    /// 用编译期 `target_arch` 判断，非运行时探测。
    pub fn current() -> Arch {
        #[cfg(target_arch = "aarch64")]
        return Arch::Aarch64;
        #[cfg(target_arch = "x86_64")]
        return Arch::X8664;
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        compile_error!("solostack 仅支持 aarch64 与 x86_64 两种架构");
    }

    /// 该架构对应的 Hadoop 官方包名称后缀（用于下载选包）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Arch::Aarch64 => "aarch64",
            Arch::X8664 => "x86_64",
        }
    }
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_returns_some_arch() {
        // 任意编译目标都应得到两种之一，且非 panic。
        let arch = Arch::current();
        assert!(matches!(arch, Arch::Aarch64 | Arch::X8664));
    }

    #[test]
    fn as_str_roundtrip() {
        assert_eq!(Arch::Aarch64.as_str(), "aarch64");
        assert_eq!(Arch::X8664.as_str(), "x86_64");
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(Arch::Aarch64.to_string(), "aarch64");
        assert_eq!(Arch::X8664.to_string(), "x86_64");
    }
}
