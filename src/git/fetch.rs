use crate::git::writer::GitWriter;

use actix_web::web::Bytes;
use anyhow::Result;
use git2::{Buf, ObjectType, Oid, Repository as Git2Repository};
use log::warn;

pub(crate) async fn fetch(input: Vec<Vec<u8>>, repo: &Git2Repository) -> Result<Bytes> {
    let mut options = Fetch::default();
    let mut writer = GitWriter::new();

    for raw_line in input.iter() {
        let line = String::from_utf8(raw_line.to_vec())?;

        if line == "thin-pack" {
            options.thin_pack = true;
        }

        if line == "no-progress" {
            options.no_progress = true;
        }

        if line == "include-tag" {
            options.include_tag = true;
        }

        if line == "ofs-delta" {
            options.ofs_delta = true;
        }

        if line.starts_with("have ") {
            options.have.push(line[5..].to_owned());
        }

        if line.starts_with("want ") {
            options.want.push(line[5..].to_owned());
        }

        /*if line.starts_with("shallow ") {
            options.shallow.push(line[8..].to_owned());
        }

        if line.starts_with("deepen ") {
            options.deepen = Some(line[7..].parse::<i32>()?);
        }

        if line == "deepen-relative" {
            options.deepen_relative = true;
        }

        if line.starts_with("deepen-since ") && options.deepen.is_none() {
            let timestamp_str = &line[13..];
            //parse timestamp_str to DateTime<Utc>
        }

        if line.starts_with("deepen-not ") && options.deepen.is_none() {
            options.deepen_not = Some(line[11..].to_owned());
        }*/

        if line == "done" {
            break;
        }
    }

    if let Some(acknowledgments) = process_haves(&repo, &options).await? {
        writer.append(acknowledgments).await?;
    }

    if let Some(wants) = process_wants(&repo, &options).await? {
        writer.append(wants).await?;
    }

    /*if let Some(mut shallows) = process_shallows(&repo, &options).await? {
        writer = writer.append(&mut shallows);
    }*/

    writer.flush().await?;

    Ok(writer.serialize().await?)
}

pub(crate) async fn process_haves(repo: &Git2Repository, options: &Fetch) -> Result<Option<GitWriter>> {
    if options.have.is_empty() {
        return Ok(None);
    }

    let mut written_one = false;
    let mut writer = GitWriter::new();
    writer.write_text("acknowledgments").await?;

    for have in &options.have {
        match repo.find_reference(have.as_str()) {
            Ok(reference) => {
                if let Some(name) = reference.name() {
                    writer.write_text(format!("ACK {}", name)).await?;
                    written_one = true;
                }
            }
            Err(e) => {
                warn!("Unable to find reference {} user has: {}", have, e);
            }
        }
    }

    if !written_one {
        writer.write_text("NAK").await?;
    }

    Ok(Some(writer))
}

pub(crate) async fn process_wants(repo: &Git2Repository, options: &Fetch) -> Result<Option<GitWriter>> {
    let mut writer = GitWriter::new();
    writer.write_text("packfile").await?;

    let mut pack_builder = repo.packbuilder()?;
    pack_builder.set_threads(num_cpus::get() as u32);

    /*pack_builder.set_progress_callback(|stage, current, total| {
        match stage {
            PackBuilderStage::AddingObjects => {
                let last_object = current == total;

                let ending = if last_object { ", done.\n" } else { "\r" };

                let percentage = current * 100 / total;
                let mut percentage_str = format!("{}", percentage);

                if percentage != 100 {
                    percentage_str = " ".to_owned() + &percentage_str;
                }

                futures::executor::block_on(writer.write_binary_ignore_err(format!("\x02Compressing objects: {}% ({}/{}){}", percentage_str, current, total, ending).as_bytes()));

            }
            PackBuilderStage::Deltafication => { /* ignored */ }
        }

        true
    })?;*/

    writer.write_text(format!("\x02Enumerating objects: {}, done.", options.want.len())).await?;

    for wanted_obj in &options.want {
        match repo.find_object(Oid::from_str(wanted_obj.as_str())?, None) {
            Ok(object) => {
                if let Some(kind) = object.kind() {
                    match kind {
                        ObjectType::Commit => {
                            pack_builder.insert_commit(object.id())?;
                        }
                        ObjectType::Tree => {
                            pack_builder.insert_tree(object.id())?;
                        }
                        _ => {
                            pack_builder.insert_object(object.id(), Some(wanted_obj.as_str()))?;
                        }
                    }
                } else {
                    pack_builder.insert_object(object.id(), Some(wanted_obj.as_str()))?;
                }
            }
            Err(e) => {
                warn!("Unable to find wanted object: {} error: {}", &wanted_obj, e);
            }
        }
    }

    let mut buf = Buf::new();
    pack_builder.write_buf(&mut buf)?;

    let buf_ref: &[u8] = buf.as_ref();
    let pack_line = [b"\x01", buf_ref].concat(); // Data gets only sent on band 1

    writer.write_binary(pack_line.as_slice()).await?;

    Ok(Some(writer))
}

/*pub(crate) async fn process_shallows(repo: &Git2Repository, options: &Fetch) -> Result<Option<GitWriter>> {
    if !repo.is_shallow() || options.shallow.is_empty() {
        return Ok(None);
    }

    let mut writer = GitWriter::new();
    writer = writer.write_text("shallow-info")?;

    // ...

    Ok(Some(writer))
}*/

pub(crate) struct Fetch {
    pub(crate) thin_pack: bool,
    pub(crate) no_progress: bool,
    pub(crate) include_tag: bool,
    pub(crate) ofs_delta: bool, // PACKv2
    pub(crate) have: Vec<String>,
    pub(crate) want: Vec<String>,
    /*pub(crate) shallow: Vec<String>,
    pub(crate) deepen: Option<i32>,
    pub(crate) deepen_relative: bool,
    pub(crate) deepen_since: Option<DateTime<Utc>>,
    pub(crate) deepen_not: Option<String>*/
}

impl Default for Fetch {
    fn default() -> Fetch {
        Fetch {
            thin_pack: false,
            no_progress: false,
            include_tag: false,
            ofs_delta: false,
            have: Vec::<String>::new(),
            want: Vec::<String>::new(),
            /*shallow: Vec::<String>::new(),
            deepen: None,
            deepen_relative: false,
            deepen_since: None,
            deepen_not: None*/
        }
    }
}