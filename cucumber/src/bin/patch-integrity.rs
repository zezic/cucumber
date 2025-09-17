use std::io::Read;
use std::{env, fs, path::Path};

use anyhow::anyhow;

use cucumber::jar::analysis::extract_general_goodies;
use cucumber::jar::reasm;
use cucumber::patching::patch_class;
use krakatau2::{
    file_output_util::Writer,
    lib::{classfile, ParserOptions},
    zip,
};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let input_jar = &args[1];
    let output_jar = &args[2];

    let file = fs::File::open(input_jar)?;
    let mut zip = zip::ZipArchive::new(file)?;

    let general_goodies = extract_general_goodies(&mut zip, |_| {})?;

    let mut file = zip.by_name(&general_goodies.init_class).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let mut class = classfile::parse(
        &buffer,
        ParserOptions {
            no_short_code_attr: true,
        },
    )
    .map_err(|err| anyhow!("Parse: {:?}", err))?;

    patch_class(&mut class);

    let patched = reasm(&class).unwrap();
    drop(file);

    let mut writer = Writer::new(Path::new(output_jar))?;

    let mut class_buf = Vec::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name().to_owned();

        if name.eq(&general_goodies.init_class) {
            writer.write(Some(&name), &patched)?;
        } else {
            class_buf.clear();
            class_buf.reserve(file.size() as usize);
            file.read_to_end(&mut class_buf)?;
            writer.write(Some(&name), &class_buf)?;
        }
    }

    Ok(())
}
