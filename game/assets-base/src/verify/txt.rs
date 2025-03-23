use std::io::BufRead;

pub fn verify_txt(file: &[u8], file_name: &str) -> anyhow::Result<()> {
    for line in file.lines() {
        match line {
            Ok(line) =>
            // also check if only allowed characters are inside the strings
            {
                for char in line.chars() {
                    if !char.is_ascii_graphic() && !char.is_ascii_whitespace() {
                        anyhow::bail!(
                            "downloaded text resource (txt) \
                            ({}) contains an unallowed character: \"{}\"",
                            file_name,
                            char
                        );
                    }
                }
            }
            Err(err) => {
                anyhow::bail!(
                    "downloaded text resource (txt) \
                    ({}) is not an allowed text file: {}",
                    file_name,
                    err
                );
            }
        }
    }

    Ok(())
}
