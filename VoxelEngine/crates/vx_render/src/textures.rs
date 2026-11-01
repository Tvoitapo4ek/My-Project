use std::path::Path;

/// Загрузить текстуры `<dir>/<name>.png` в RGBA-слои одинакового размера.
/// Отсутствующие или неподходящие файлы заменяются шахматкой — видно сразу.
pub fn load_layers(dir: &Path, names: &[String]) -> (u32, Vec<Vec<u8>>) {
    let images: Vec<_> = names
        .iter()
        .map(|n| {
            let path = dir.join(format!("{n}.png"));
            match image::open(&path) {
                Ok(img) => Some(img.to_rgba8()),
                Err(e) => {
                    log::warn!("текстура `{}`: {e}", path.display());
                    None
                }
            }
        })
        .collect();
    let size = images.iter().flatten().map(|i| i.width()).next().unwrap_or(16);
    let mut layers: Vec<Vec<u8>> = images
        .into_iter()
        .zip(names)
        .map(|(img, name)| match img {
            Some(i) if i.width() == size && i.height() == size => i.into_raw(),
            Some(_) => {
                log::warn!("текстура `{name}`: все текстуры должны быть {size}×{size}");
                checker(size)
            }
            None => checker(size),
        })
        .collect();
    // Массив из одного слоя на некоторых бэкендах становится обычной 2D-текстурой.
    while layers.len() < 2 {
        layers.push(checker(size));
    }
    (size, layers)
}

fn checker(size: u32) -> Vec<u8> {
    (0..size * size)
        .flat_map(|i| {
            let (x, y) = (i % size, i / size);
            if (x * 2 / size + y * 2 / size).is_multiple_of(2) { [255, 0, 255, 255] } else { [0, 0, 0, 255] }
        })
        .collect()
}
