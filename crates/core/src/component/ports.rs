//! 端口工具:默认值回退与占用避让。
//!
//! 端口分配是「机制通用」的共享设施 —— 各组件只提供自己的默认端口与需要绕开的
//! 保留端口，策略（用户显式指定优先、默认值被占则递增、跳过保留端口）只此一份。

/// 端口避让：自定义值(≠默认)尊重；默认值被占用则递增到空闲。
pub fn pick_free(preferred: u16, default: u16) -> u16 {
    pick_free_excluding(preferred, default, &[])
}

/// 同 `pick_free`，并额外跳过 `reserved` 里的端口（组件内部固定占用的端口）。
///
/// 例：Kafka 的 controller 端口 9093 固定占用，broker 端口必须绕开它 ——
/// 否则会写出两个监听器绑同一端口，Kafka 直接起不来。
pub fn pick_free_excluding(preferred: u16, default: u16, reserved: &[u16]) -> u16 {
    if preferred != default {
        return preferred;
    }
    let mut p = preferred;
    while (reserved.contains(&p) || crate::platform::process::port_open(p)) && p < u16::MAX {
        p += 1;
    }
    p
}

/// 校验一组「角色名 → 端口」互不重复。
///
/// 同一组件内部端口撞车（含用户手填、避让后偶然撞上）会让组件绑不上第二个监听器，
/// 报错往往很含糊；这里提前给出指名道姓的提示。
pub fn ensure_distinct(ports: &[(&str, u16)]) -> Result<(), String> {
    for (i, (name_a, port_a)) in ports.iter().enumerate() {
        for (name_b, port_b) in ports.iter().skip(i + 1) {
            if port_a == port_b {
                return Err(format!("端口冲突：{name_a} 与 {name_b} 都是 {port_a}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_distinct_reports_the_colliding_pair() {
        assert!(ensure_distinct(&[("a", 1), ("b", 2)]).is_ok());
        let err = ensure_distinct(&[("namenode", 9870), ("yarn", 9870)]).unwrap_err();
        assert!(err.contains("namenode") && err.contains("yarn") && err.contains("9870"));
    }

    #[test]
    fn explicit_port_is_returned_unchanged() {
        // 用户显式指定（≠默认）时不做避让，也不跳保留端口（是否合法由调用方校验）
        assert_eq!(pick_free(12345, 9870), 12345);
        assert_eq!(pick_free_excluding(12345, 9870, &[12345]), 12345);
    }
}
