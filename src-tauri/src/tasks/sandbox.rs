//! 工作目录检查（程序侧强制执行，不只靠提示词）：
//! - 任务只能在「任务根目录」或它的子目录里运行；
//! - 任务根目录本身不能是系统目录、盘符根目录、用户主目录、DeskMate 的数据目录等敏感位置；
//! - 路径先规范化（解析 `..`、符号链接 / 目录联接），防止绕出去。

use std::path::{Component, Path, PathBuf};

/// 去掉 Windows 规范化路径的 `\\?\` 前缀，便于比较和显示
pub fn simplify(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

fn lower(p: &Path) -> String {
    simplify(p)
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// `inner` 是否等于或位于 `outer` 之内（按路径组件比较，大小写不敏感）
pub fn is_within(inner: &Path, outer: &Path) -> bool {
    let i = lower(inner);
    let o = lower(outer);
    i == o || i.starts_with(&format!("{o}\\"))
}

fn is_drive_root(p: &Path) -> bool {
    let simple = simplify(p);
    let comps: Vec<Component> = simple.components().collect();
    comps
        .iter()
        .all(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
}

/// 不允许作为任务根目录（或包含它们）的位置
fn forbidden_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for var in [
        "SystemRoot",
        "windir",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
        "APPDATA",
        "LOCALAPPDATA",
    ] {
        if let Some(x) = std::env::var_os(var) {
            v.push(PathBuf::from(x));
        }
    }
    v
}

/// 检查任务根目录是否安全（设置界面保存时调用）
pub fn validate_root(root: &Path, data_dir: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute() {
        return Err("任务文件夹必须是完整路径，比如 D:\\DeskMate-Work".into());
    }
    std::fs::create_dir_all(root).map_err(|e| format!("无法创建任务文件夹：{e}"))?;
    let canon =
        simplify(&std::fs::canonicalize(root).map_err(|e| format!("无法访问任务文件夹：{e}"))?);
    // 数据目录也换成完整路径（同一个文件夹可能有 8.3 短名等不同写法）
    let data = std::fs::canonicalize(data_dir)
        .map(|d| simplify(&d))
        .unwrap_or_else(|_| data_dir.to_path_buf());
    check_root_location(&canon, &data)?;
    Ok(canon)
}

/// 纯路径规则（不访问磁盘，便于测试）
pub fn check_root_location(canon: &Path, data_dir: &Path) -> Result<(), String> {
    if is_drive_root(canon) {
        return Err("不能把整个磁盘作为任务文件夹，请新建一个专用文件夹".into());
    }
    if is_within(data_dir, canon) || is_within(canon, data_dir) {
        return Err("不能使用 DeskMate 自己的数据文件夹".into());
    }
    for f in forbidden_roots() {
        if is_within(canon, &f) || is_within(&f, canon) {
            return Err(format!("不能使用系统或程序文件夹（{}）", f.display()));
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from)
        && (lower(canon) == lower(&home) || is_within(&home, canon))
    {
        return Err("不能把整个用户文件夹作为任务文件夹，请在里面新建一个专用文件夹".into());
    }
    // 只有一层（比如 C:\Users）也太宽了
    if simplify(canon)
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count()
        == 0
    {
        return Err("任务文件夹太宽泛了".into());
    }
    Ok(())
}

/// 检查某次任务的工作目录：必须存在，且位于任务根目录之内
pub fn validate_workdir(dir: &Path, root: &Path) -> Result<PathBuf, String> {
    if !dir.is_absolute() {
        return Err("工作目录必须是完整路径".into());
    }
    let canon_dir = simplify(
        &std::fs::canonicalize(dir).map_err(|_| format!("找不到文件夹：{}", dir.display()))?,
    );
    let canon_root = simplify(
        &std::fs::canonicalize(root).map_err(|_| "任务文件夹不存在，请在设置里检查".to_string())?,
    );
    if !canon_dir.is_dir() {
        return Err("工作目录不是文件夹".into());
    }
    if !is_within(&canon_dir, &canon_root) {
        return Err(format!(
            "为了安全，任务只能在任务文件夹（{}）里面进行。可以把文件先放进去，或者在设置里修改任务文件夹。",
            canon_root.display()
        ));
    }
    Ok(canon_dir)
}

/// 默认的任务根目录：有 D 盘就用 D:\DeskMate-Work，否则用 用户目录\DeskMate-Work
pub fn default_root() -> PathBuf {
    if Path::new("D:\\").exists() {
        PathBuf::from("D:\\DeskMate-Work")
    } else {
        std::env::var_os("USERPROFILE")
            .map(|h| PathBuf::from(h).join("DeskMate-Work"))
            .unwrap_or_else(|| PathBuf::from("DeskMate-Work"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_is_component_based_and_case_insensitive() {
        assert!(is_within(Path::new(r"D:\Work\a"), Path::new(r"d:\work")));
        assert!(is_within(Path::new(r"D:\Work"), Path::new(r"D:\Work\")));
        assert!(!is_within(Path::new(r"D:\Work2"), Path::new(r"D:\Work")));
        assert!(!is_within(Path::new(r"D:\"), Path::new(r"D:\Work")));
        assert!(is_within(
            Path::new(r"\\?\D:\Work\x"),
            Path::new(r"D:\Work")
        ));
    }

    #[test]
    fn root_location_rules() {
        let data = Path::new(r"C:\Users\me\AppData\Roaming\DeskMate");
        assert!(check_root_location(Path::new(r"D:\"), data).is_err());
        assert!(
            check_root_location(Path::new(r"C:\Users\me\AppData\Roaming\DeskMate"), data).is_err()
        );
        assert!(
            check_root_location(Path::new(r"C:\Users\me\AppData\Roaming"), data).is_err(),
            "包含数据目录"
        );
        assert!(check_root_location(Path::new(r"D:\DeskMate-Work"), data).is_ok());
        if let Some(win) = std::env::var_os("SystemRoot") {
            assert!(check_root_location(&PathBuf::from(win).join("Temp"), data).is_err());
        }
    }

    #[test]
    fn workdir_must_be_inside_root_even_via_dotdot() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("work");
        let inside = root.join("proj");
        let outside = t.path().join("secret");
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        assert!(validate_workdir(&inside, &root).is_ok());
        assert!(validate_workdir(&root, &root).is_ok());
        assert!(validate_workdir(&outside, &root).is_err());
        assert!(
            validate_workdir(&root.join("..").join("secret"), &root).is_err(),
            "用 .. 绕出去会被发现"
        );
        assert!(
            validate_workdir(&root.join("nope"), &root).is_err(),
            "不存在的目录"
        );
        assert!(validate_workdir(Path::new("relative"), &root).is_err());
    }
}
