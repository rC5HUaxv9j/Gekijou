use std::io::Cursor;

use regex::Regex;
use reqwest;
use serde::{Deserialize, Serialize};
use xml;

use crate::{file_name_recognition, GLOBAL_ANIME_DATA};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RssEntry {
    pub title: String,
    pub link: String,
    pub guid: String,
    pub pub_date: String,
    pub downloads: i32,
    pub info_hash: String,
    pub category_id: String,
    pub size: i32,
    pub size_string: String,

    pub derived_values: DerivedValues,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DerivedValues {
    pub episode: i32,
    pub resolution: i32,
    pub sub_group: String,
    pub anime_id: i32,
    pub title: String,
    pub batch: bool,
}

pub async fn get_rss(anime_id: i32) -> Vec<RssEntry> {

    let anime_data = GLOBAL_ANIME_DATA.lock().await;
    let search = anime_data.get(&anime_id).unwrap().title.romaji.clone().unwrap().replace(" ", "+");
    let url = format!("https://nyaa.si/?page=rss&q={}&c=1_2&f=0", search);

    let response = reqwest::get(url).await.unwrap().text().await.unwrap()
        .replace("\n", "")
        .replace("\t", "");
    //println!("{}", response);

    let cursor = Cursor::new(response);
    
    // Parse the XML document
    let doc = xml::reader::EventReader::new(cursor);

    // Iterate through the events in the XML document
    let mut entry: RssEntry = RssEntry::default();
    let mut entrys: Vec<RssEntry> = Vec::new();

    let mut element_name = String::new();
    for event in doc {
        match event {
            Ok(xml::reader::XmlEvent::StartElement { name, attributes: _, .. }) => {
                element_name = name.local_name;
            }
            Ok(xml::reader::XmlEvent::Characters(text)) => {
                match element_name.as_str() {
                    "title" => { entry.title = text; },
                    "link" => { entry.link = text; },
                    "guid" => { entry.guid = text; },
                    "pubDate" => { entry.pub_date = text; },
                    "downloads" => { entry.downloads = text.parse().unwrap(); },
                    "infoHash" => { entry.info_hash = text; },
                    "categoryId" => { entry.category_id = text; },
                    "size" => { entry.size_string = text; },
                    &_ => (),
                }
            }
            Ok(xml::reader::XmlEvent::EndElement { name }) => {
                if name.local_name == "item" {
                    entrys.push(entry);
                    entry = RssEntry::default();
                }
                element_name = String::new();
            }
            _ => {}
        }
    }

    let valid_file_extensions = Regex::new(r"[_ ]?(\.mkv|\.avi|\.mp4)").unwrap();
    let file_size = Regex::new(r"(\d{1,3}\.\d?)").unwrap();
    for e in entrys.iter_mut() {

        let mut title = e.title.clone();
        
        title = valid_file_extensions.replace_all(&title, "").to_string();

        e.derived_values.resolution = file_name_recognition::extract_resolution(&title);

        e.derived_values.sub_group = file_name_recognition::extract_sub_group(&title);

        title = file_name_recognition::remove_brackets(&title);

        let (episode_string, episode) = file_name_recognition::identify_number(&title);
        e.derived_values.episode = episode;
        title = title.replace(&episode_string, "");

        let lowercase_title = title.to_ascii_lowercase();
        let (mut identified_anime_id, mut identified_title, mut similarity) = file_name_recognition::identify_media_id(&lowercase_title, &anime_data, Some(anime_id));
        //println!("{} {} {} ", identified_anime_id, identified_title, similarity);
        if identified_anime_id == 0 {
            (identified_anime_id, identified_title, similarity) = file_name_recognition::identify_media_id(&lowercase_title, &anime_data, None);
            //print!("{} {} {} ", identified_anime_id, identified_title, similarity)
        }
        if similarity > 0.0 {
            e.derived_values.anime_id = identified_anime_id;
            e.derived_values.title = identified_title;
        }

        let captures = file_size.captures(&e.size_string).unwrap();
        let size: f64 = captures.get(1).unwrap().as_str().parse().unwrap();

        if e.size_string.contains("GiB") {
            e.size = (size * 1024.0 * 1024.0) as i32;
        } else if e.size_string.contains("MiB") {
            e.size = (size * 1024.0) as i32;
        } else {
            e.size = size as i32;
        }

        e.derived_values.batch = identify_batch(&e.title, e.derived_values.episode, e.size);

        //println!("{}|{}|{}", e.title, e.derived_values.episode, e.size);
    }

    entrys
}


fn identify_batch(filename: &String, episode: i32, size: i32) -> bool {

    let season = Regex::new(r"[Ss]eason ?\d+").unwrap();
    let season_short = Regex::new(r"[Ss] ?\d").unwrap();
    let season_short_not = Regex::new(r"[Ss] ?\d+[Ee]\d+").unwrap();
    let episode_range = Regex::new(r"0?1 ?[-~] ?\d+").unwrap();
    let batch = Regex::new(r"[Bb]atch").unwrap();

    if batch.is_match(filename){
        return true;
    }
    if episode_range.is_match(filename){
        return true;
    }
    if season.is_match(filename){
        return true;
    }
    if season_short.is_match(filename) && season_short_not.is_match(filename) == false {
        return true;
    }
    if episode == 0 {
        return true;
    }
    if size > 3 * 1024 * 1024 /* 3GB */ {
        return true;
    }

    false
}