//! Retention: удаление файлов записи старше N дней. Фоновое расписание —
//! задача рекордера (T5); здесь — операция очистки над директорией данных.

use crate::StoreError;
use crate::day::{day_of, parse_day};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Удалить файлы записи старше `retention_days` UTC-дней от `now_us`.
/// Возраст файла — по дате в его имени (`YYYY-MM-DD.parquet`), не по
/// времени файловой системы; файлы, не похожие на имя записи, не
/// трогаются. Возвращает пути удалённых файлов. Отсутствующая директория
/// — не ошибка: очистка до первой записи штатна. Файл с возрастом ровно
/// `retention_days` остаётся: «старше» — строго больше.
pub fn prune(
    data_dir: &Path,
    now_us: i64,
    retention_days: u64,
) -> Result<Vec<PathBuf>, StoreError> {
    let today = day_of(now_us);
    let mut deleted = Vec::new();
    walk(data_dir, today, retention_days, &mut deleted)?;
    Ok(deleted)
}

fn walk(
    dir: &Path,
    today: i64,
    retention_days: u64,
    deleted: &mut Vec<PathBuf>,
) -> Result<(), StoreError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            walk(&path, today, retention_days, deleted)?;
        } else if file_type.is_file() {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(day) = name.strip_suffix(".parquet").and_then(parse_day) else {
                continue;
            };
            if today - day > i64::try_from(retention_days).unwrap_or(i64::MAX) {
                fs::remove_file(&path)?;
                deleted.push(path);
            }
        }
    }
    Ok(())
}
