// Include the uniffi scaffolding file.
uniffi::include_scaffolding!("didcomm_communications");

// External resources.
extern crate didcomm_rs;
extern crate serde_json;

// Rust elements.
use std::io::{Write, BufReader};
use std::fs;
use std::fs::File;
use std::path::Path;
use std::str;
use std::sync::{Arc, RwLock};

// External crates.
use aes::{Aes256};
use arrayref::array_ref;
use block_modes::{BlockMode, Cbc};
use block_modes::block_padding::Pkcs7;
use bson::{bson, doc, Document};
use filetime::{FileTime, set_file_mtime};
use hmac::{Hmac, Mac};
use rust_base58::{ToBase58, FromBase58};
use serde::{Deserialize, Serialize};
use serde_json::{Deserializer, Value};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

// DIDComm resources.
use didcomm_rs::{Message};
use didcomm_rs::crypto::{CryptoAlgorithm};

// Custom types (simplifies use in coding).
type Aes256Cbc = Cbc<Aes256, Pkcs7>;
type HmacSha256 = Hmac<Sha256>;

// Constants
const MAX_PATH_LENGTH: usize = 1024;
const AES256_KEY_LENGTH: usize = 32;
const AES256_IV_LENGTH: usize = 16;
const HMACSHA256_LENGTH: usize = 32;
const CURVE25519_KEY_LENGTH: usize = 32;

// ---- Structures ----
// KeyPair
#[derive(Debug)]
pub struct KeyPair {
    pub public_key: RwLock<Vec<u8>>,
    pub private_key: RwLock<Vec<u8>>,
    pub did: RwLock<String>,
}

impl KeyPair {
    pub fn new(initial_private_key: String) -> Self {

        let k = match Arc::try_unwrap(generate_key_pair(initial_private_key)) {
            Ok(x) => x,
            Err(_) => KeyPair {
                public_key: RwLock::new([0].to_vec()),
                private_key: RwLock::new([0].to_vec()),
                did: RwLock::new("0".to_string())
            }
        };
        return k;
    }

    pub fn get_public_key(&self) -> String {
        return self.public_key.read().unwrap().to_base58();
    }

    pub fn get_private_key(&self) -> String {
        return self.private_key.read().unwrap().to_base58();
    }

    pub fn get_did(&self) -> String {
        return self.did.read().unwrap().clone();
    }
}

// FileHeader
#[derive(Serialize, Deserialize)]
pub struct FileHeader {
    encrypted_filename: String,
    filename_hmac256: String,
    key_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct FilePayload {
    file_name: String,
    file_data: Vec<u8>
}

// --------------------------------------------------------------------------
// The following are helper functions used to assist with languages that 
// don't have unsigned values (e.g., Java).

// --------------------------------------------------------------------------
// convert_i16_to_u8_vec ----
fn convert_i16_to_u8_vec(input: Vec<i16>) -> Vec<u8> {

    let mut output : Vec<u8> = Vec::new();
    for val in input.iter() {
        output.push((val & 0xFF) as u8);
    }

    return output;
}

// --------------------------------------------------------------------------
// convert_i16_to_u8_vec ----
fn convert_i8_to_u8_vec(input: Vec<i8>) -> Vec<u8> {

    let mut output : Vec<u8> = Vec::new();
    for val in input.iter() {
        output.push(*val as u8);
    }

    return output;
}

// --------------------------------------------------------------------------
// generate_key_pair()
pub fn generate_key_pair(initial_private_key: String) -> Arc<KeyPair> {

    // Start with a base58 encoding of a private key bytes.
    let private_key = initial_private_key
        .from_base58()
        .unwrap();

    // Turn it into a StaticSecret.
    let secret_key: StaticSecret = StaticSecret::from(array_ref!(private_key, 0, AES256_KEY_LENGTH).to_owned());

    // Use it to generate the PublicKey.
    let public_key = PublicKey::from(&secret_key);

    // Now, get the public & private keys bytes as byte arrays.
    let public: [u8; CURVE25519_KEY_LENGTH] = public_key.to_bytes();
    let private: [u8; CURVE25519_KEY_LENGTH] = secret_key.to_bytes();

    // The DID contains the public key.  
    // SECURITY NOTE:  this did is hardcoded for this tutorial.  In practice, 
    // it will be generated along with the keys.
    const MY_DID: &str = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";

    return Arc::new(KeyPair {
            public_key: RwLock::new(public.to_vec()), 
            private_key: RwLock::new(private.to_vec()), 
            did: RwLock::new(MY_DID.to_string())
        });
}

// --------------------------------------------------------------------------
// generate_iv()
fn generate_iv(init_string: String) -> [u8; AES256_IV_LENGTH] {

    // SECURITY NOTE:  initialization vectors MUST be randomly generated.  For 
    // the purposes of this tutorial, it is intentionally set to always be 0's.  
    // This is for simplicity and predictability in debugging and testing.  
    // In practice, ***DO NOT DO THIS*** and instead generate a random IV.
    let mut iv: [u8; 16] = [0u8; AES256_IV_LENGTH];
    let i = init_string.from_base58().unwrap();
    iv[..AES256_IV_LENGTH].copy_from_slice(&i[..AES256_IV_LENGTH]);

    return iv;
}

// --------------------------------------------------------------------------
// SECURITY NOTE:  This function simulates one of the functions of a 
// decentralized identity wallet.  It accepts a DID and looks up a corresponding 
// AES key.  For simplicity in this tutorial, it always returns a default 
// AES key of 0's.  In practice, this process would return / generate a properly 
// generated AES key, that would also be stored in a decentralized identity wallet.
fn get_aes_key(_did: String) -> [u8; AES256_KEY_LENGTH] {

    return [0; AES256_KEY_LENGTH];
}

// --------------------------------------------------------------------------
// encrypt_filename() 
pub fn encrypt_filename(filename: String, key: [u8; AES256_KEY_LENGTH], iv: [u8; AES256_IV_LENGTH]) -> Vec<u8> {

    return match Aes256Cbc::new_from_slices(&key, &iv){
        Ok(cipher) => {
            let plaintext = filename.as_bytes();

            // Encrypt the allowable number of bytes ... either the filename length or the MAX_PATH_LENGTH.
            let len = std::cmp::min(plaintext.len(), MAX_PATH_LENGTH);
            let mut buffer = [0u8; MAX_PATH_LENGTH];
            buffer[..len].copy_from_slice(&plaintext[..len]);
            let ciphertext = cipher.encrypt(&mut buffer, len).unwrap();

            ciphertext.to_vec()
        },  
        Err(_e) => {
            let r_value: Vec<u8> = Vec::new();
            r_value
        },
    };
} 

// --------------------------------------------------------------------------
// decrypt_filename()
pub fn decrypt_filename(file_header: FileHeader) -> String {

    let iv: [u8; AES256_IV_LENGTH] = generate_iv(file_header.filename_hmac256.clone());

    let plaintext: String = match Aes256Cbc::new_from_slices(&file_header.key_id.from_base58().unwrap(), iv.as_slice()) {
        Ok(cipher) => {
            let mut data = file_header.encrypted_filename.from_base58().unwrap();
            let plaintext = cipher.decrypt(&mut data).unwrap();
            str::from_utf8(plaintext).unwrap().to_string()
        },
        Err(_) => {
            "".to_string()
        }
    };

    return plaintext;
}

// --------------------------------------------------------------------------
// hash_filename()
pub fn hash_filename(filename: String, key: String) -> String {

    let mut hmac = HmacSha256::new_from_slice(&key.from_base58().unwrap())
        .expect("HMAC needs a valid key.");
    hmac.update(&filename.clone().into_bytes());
    let hmac_result = hmac.finalize();

    let hmac_bytes: [u8; HMACSHA256_LENGTH] = hmac_result.into_bytes().as_slice().try_into().expect("Wrong length");
    let hashed_filename = hmac_bytes.as_slice().to_base58();

    return hashed_filename;
} 

// --------------------------------------------------------------------------
// encrypt_file_u16()
// Note:  convenience method for converting data array types.  
// Calls encrypt_file_u8().
pub fn encrypt_file_i16(
    sender_did: String,
    sender_priv_key: String,
    recipient_did: String, 
    recipient_pub_key: String, 
    filename: String,
    file_data: Vec<i16>,
    source_root: String,
    dest_root: String) -> String {

    encrypt_file_u8(
        sender_did, sender_priv_key,
        recipient_did, recipient_pub_key, 
        filename,
        convert_i16_to_u8_vec(file_data), 
        source_root, 
        dest_root)
}

// --------------------------------------------------------------------------
// encrypt_file_i8()
// Note:  convenience method for converting data array types.  
// Calls encrypt_file_u8().
pub fn encrypt_file_i8(
    sender_did: String,
    sender_priv_key: String,
    recipient_did: String, 
    recipient_pub_key: String, 
    filename: String,
    file_data: Vec<i8>,
    source_root: String,
    dest_root: String) -> String {

    encrypt_file_u8(
        sender_did, sender_priv_key,
        recipient_did, recipient_pub_key, 
        filename,
        convert_i8_to_u8_vec(file_data), 
        source_root, 
        dest_root)
}

// --------------------------------------------------------------------------
// encrypt_file_u8()
pub fn encrypt_file_u8(
    sender_did: String,
    sender_priv_key: String,
    recipient_did: String, 
    recipient_pub_key: String, 
    filename: String,
    file_data: Vec<u8>,
    source_root: String,
    dest_root: String) -> String {

    let file_rel_path = filename.replace(&source_root, "");

    // Build the file header.
    let key_id: [u8; AES256_KEY_LENGTH] = get_aes_key(sender_did.clone());
    let header = build_file_header(file_rel_path.clone(), key_id);

    // Create output file name.
    let filename_hash:  String = std::path::MAIN_SEPARATOR_STR.to_owned() + &header.filename_hmac256;

    // Create the output file path by swapping the source & root directory paths and adding the filename_hash.
    let output_filename_hash: String = dest_root + &filename_hash;

    // Overwrite if newer.
    if is_newer(filename.clone(), output_filename_hash.clone()) || !Path::new(&output_filename_hash).exists() {

        // Add the file name and contents as the payload_data.
        let payload_data = bson!({
            "file_name" : file_rel_path.clone(),
            "file_data" : bson::to_bson(&file_data).unwrap()
        });
        
        // Create a DIDComm message.
        let message: Message = Message::new()
            .from(&sender_did)
            .to(&[&recipient_did])
            .body(&serde_json::to_string(&payload_data).unwrap()) 
            .as_jwe(
                &CryptoAlgorithm::A256GCM,
                Some(recipient_pub_key.from_base58().unwrap()));

        // Encrypt the DIDComm message.
        let sealed_message_result = message.seal(
            &sender_priv_key.from_base58().unwrap(),
            Some(vec![Some(recipient_pub_key.from_base58().unwrap())]),
        );
        let encrypted_message = match sealed_message_result {
            Ok(value) => value,
            Err(error) => error.to_string(),
        };
        
        // Create the output file and write the header and the DIDComm message.
        let mut output_file = File::create(output_filename_hash.clone()).unwrap();
        output_file.write(&serde_json::to_string(&header).unwrap().into_bytes()).unwrap();
        output_file.write(&encrypted_message.clone().into_bytes()).unwrap();

        sync_modification_times(filename, output_filename_hash.clone());
    }

    return output_filename_hash;
}

// --------------------------------------------------------------------------
// delete_plaintext_file()
pub fn delete_plaintext_file(
    sender_did: String,
    filename: String,
    source_root: String,
    dest_root: String) -> () {

    let file_rel_path = filename.replace(&source_root, "");

    // Build the file header.
    let key_id: [u8; AES256_KEY_LENGTH] = get_aes_key(sender_did);
    let header = build_file_header(file_rel_path.clone(), key_id);

    // Create output file name.
    let filename_hash:  String = std::path::MAIN_SEPARATOR_STR.to_owned() + &header.filename_hmac256;

    // Create the output file path by swapping the source & root directory paths and adding the filename_hash.
    let output_filename_hash: String = dest_root + &filename_hash;

    // Delete both files.
    let _ = fs::remove_file(filename.clone());
    let _ = fs::remove_file(output_filename_hash.clone());
}

// --------------------------------------------------------------------------
fn plaintext_file_associated_with_encrypted_file(encrypted_filename: String, source_root: String, 
    dest_root: String, key_id: [u8; AES256_KEY_LENGTH]) -> String {

    let mut file_to_delete: String = "".to_string();

    // Scan the plaintext directory.  For now, this only scans the directory.  
    // and not sub-directories.  Eventually, this will scan the entire sub-directory tree.
    let paths = fs::read_dir(dest_root.clone()).unwrap();
    for path in paths {
        if let Ok(p) = path {
            // Get a String representation of the path.
            let filepath = p.path().clone().into_os_string().into_string().unwrap();

            // Remove the full path component to get the relative path portion.
            let file_rel_path = filepath.replace(&dest_root, "");

            // Create a hash of the relative file name and create mock-up of a would be encrypted file.
            // This enables the calculated_encrypted_filename to be compared with the actual encrypted_filename that was deleted.
            let hashed_filename = hash_filename(file_rel_path.clone(), key_id.to_base58().clone());
            let calculated_encrypted_filename = source_root.clone() + &std::path::MAIN_SEPARATOR_STR.to_owned() + &hashed_filename;
            if encrypted_filename.to_string() == calculated_encrypted_filename.to_string() {
                file_to_delete = filepath;
                break;
            }
        }
    }

    return file_to_delete;
}

// --------------------------------------------------------------------------
// delete_encrypted_file()
pub fn delete_encrypted_file(
    filename: String,
    source_root: String,
    dest_root: String) -> () {

    // Read the header.
    match read_file_header(filename.clone()) {
        Some(header) => {
            // In this case, the encrypted file was found, header read and the 
            // corresponding plaintext file can be found and deleted.
            let decrypted_filename = decrypt_filename(header);
            let decrypted_filename_path: String = source_root.clone() + &decrypted_filename;
        
            // Delete both files.
            let _ = fs::remove_file(filename.clone());
            let _ = fs::remove_file(decrypted_filename_path.clone());    
        },
        None => {
            // In this case, the header could not be read, which usually means 
            // that the encrypted file was not found.  When using cloud storage 
            // services, often a remote user device will delete a plaintext file.  
            // When this happens, the cloud service will replicate the deletion
            // of the encrypted file on the other user devices.  When the monitoring
            // app discovers that an encrypted file has been deleted, it will notify 
            // this library to delete the plaintext file.  However, this case presents 
            // the dilemma that there is no encrypted file from which to read the 
            // file header and discover the plaintext file.  In this case, another method
            // must be used to delete the plaintext file. 
            let main_encryption_key: [u8; AES256_KEY_LENGTH] = get_aes_key("test".to_string());
            let plaintext_file = plaintext_file_associated_with_encrypted_file(filename.clone(), source_root.clone(), 
                dest_root.clone(), main_encryption_key);
        
            // Delete both files.
            let _ = fs::remove_file(filename.clone());
            let _ = fs::remove_file(plaintext_file.clone());    
        }
    };
}

// --------------------------------------------------------------------------
// build_file_header()
fn build_file_header(filename: String, key_id: [u8; AES256_KEY_LENGTH]) -> FileHeader {

    // Hash the filename.
    let filename_hmac256 = hash_filename(filename.clone(), key_id.to_base58());

    // Create the iv from the first 16 bytes of the filename_hmac256.
    let iv: [u8; AES256_IV_LENGTH] = generate_iv(filename_hmac256.clone());
    
    // Encrypt the filename.
    let encrypted_filename = encrypt_filename(filename, key_id, iv);
    
    // Create the FileHeader.
    let header: FileHeader = { FileHeader{
        encrypted_filename: encrypted_filename.clone().as_slice().to_base58(),
        filename_hmac256: filename_hmac256,
        key_id: key_id.to_base58(),
    }};

    return header;
}

// --------------------------------------------------------------------------
// read_file_header()
fn read_file_header(input: String) -> Option<FileHeader> {

    if Path::new(&input).exists() {
        // Open the input file.
        let file = File::open(input).unwrap();
        let mut reader = BufReader::new(file);

        // Read the header as a JSON object.
        let mut stream = Deserializer::from_reader(&mut reader).into_iter::<Value>();

        let header_string: String = match stream.next().unwrap() {
            Ok(header) => {
                header.to_string()
            },
            Err(error) => error.to_string(),
        };

        let header: FileHeader = serde_json::from_str(&header_string).unwrap();
        Some(header)
    } else {
        None
    }
}

// --------------------------------------------------------------------------
// read_file_payload()
pub fn read_file_payload(filepath: String) -> String { //FilePayload {

    // Open the input file.
    let file = File::open(filepath).unwrap();
    let mut reader = BufReader::new(file);

    // Read the header and payload body as JSON objects.
    let mut stream = Deserializer::from_reader(&mut reader).into_iter::<Value>();

    // Skip the header.
    stream.next();

    // Now read the payload.
    let payload_string: String = match stream.next().unwrap() {
        Ok(payload) => {
            payload.to_string()
        },
        Err(error) => error.to_string(),
    };

    return payload_string;
}

// --------------------------------------------------------------------------
// is_newer()
fn is_newer(input_file: String, output_file: String) -> bool {

    let mut result: bool = false;

    if Path::new(&input_file).exists() {

        if Path::new(&output_file).exists() {

            // Get input file metadata.
            let input_file_metadata = fs::metadata(input_file.clone()).unwrap();
            let i_time = FileTime::from_last_modification_time(&input_file_metadata);

            // Get output file metadata.
            let output_file_metadata = fs::metadata(output_file).unwrap();
            let o_time = FileTime::from_last_modification_time(&output_file_metadata);

            // Is input newer?
            if i_time > o_time { 
                result = true;
            }
        } else {
            result = true;
        }
    } 

    return result;
}

// --------------------------------------------------------------------------
// sync_modification_times()
fn sync_modification_times(input: String, output: String) -> () {

    // Get input file metadata.
    let input_file_metadata = fs::metadata(input.clone()).unwrap();
    let i_time = FileTime::from_last_modification_time(&input_file_metadata);

    set_file_mtime(output, i_time).unwrap();
}

// --------------------------------------------------------------------------
// decrypt_file_message()
pub fn decrypt_file_message(
    filepath: String,
    private_key: String,
    public_key: String,
    dest_root: String) -> String {

    let mut result: String = "".to_string();

    // Read the header.
    let header: FileHeader = match read_file_header(filepath.clone()) {
        Some(h) => h,
        None => return "".to_string()
    };

    let decrypted_filename = decrypt_filename(header);
    let output_file_path: String = dest_root.clone() + &decrypted_filename;

    // Overwrite if newer.
    if is_newer(filepath.clone(), output_file_path.clone()) {

        // Read the payload.
        let payload: String = read_file_payload(filepath.clone());

        // Decrypt the message (contained in the payload).
        let message = Message::receive(
            &payload,
            Some(&private_key.from_base58().unwrap()),
            Some(public_key.from_base58().unwrap()),
            None,
        );

        result = match message {
            Ok(value) => {                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                

                // Convert the decrypted message body into a FilePayload structure.
                let val_body = value.get_body().unwrap();
                let doc: Document = serde_json::from_str(&val_body).unwrap();
                let payload_data: FilePayload = bson::from_document(doc).unwrap();

                // Create the output file.
                let output_file_path: String = dest_root.clone() + &payload_data.file_name;
                let mut output_file = File::create(output_file_path.clone()).unwrap();

                // Write contents to the output file.
                output_file.write(&payload_data.file_data.clone()).unwrap();

                sync_modification_times(filepath, output_file_path.clone());

                // Return the new file path.
                output_file_path.clone()
            },
            Err(error) => {
                error.to_string();
                "".to_string()
            }
        };
    } 

    return result;
}   

// --------------------------------------------------------------------------
// Test methods.
#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time};

    #[test]
    fn it_works() {

        println!("\n\n---- from generate_key_pair() ----\n");
        let k = generate_key_pair("6QN8DfuN9hjgHgPvLXqgzqYE3jRRGRrmJQZkd5tL8paR".to_string());
        println!("     did = {:?}", k.get_did());
        println!("     pubkey = {:?}", k.get_public_key());
        println!("     privkey = {:?}", k.get_private_key());

        // Simulate reading data from a file, because the file may not exist.
        println!("\n\n---- test data ----\n");
        let file_name = "/Users/mccown/test1/file1.txt";
        let file_data: Vec<i16> = [0x41, 0x41, 0x41, 0xEA].to_vec();
        println!("     File name = {}", file_name);
        println!("     Plaintext input data = {:?}", file_data);

        println!("\n\n---- from encrypt_message() ----\n");
        let source_root: String = "/Users/mccown/test1".to_string();
        let dest_root: String = "/Users/mccown/test2".to_string();
        let enc_file_path = encrypt_file_i16(
            k.get_did(), 
            k.get_private_key(), 
            k.get_did(), 
            k.get_public_key(),
            file_name.to_string(), 
            file_data.clone(),
            source_root.clone(),
            dest_root.clone()
        );
        println!("     Encrypted file path = {:?}", enc_file_path);

        println!("\n\n---- from decrypt_file_message() ----\n");
        let output_file = decrypt_file_message(
            enc_file_path,
            k.get_private_key(),
            k.get_public_key(),
            source_root.clone()
        );
        println!("      Decrypted output file = {}", output_file);

        // ---- Deletes ----
        let file_name = "/Users/mccown/test1/file1.txt";
        let file_name_copy_1 = "/Users/mccown/test1/file1_1.txt";
        let file_name_copy_2 = "/Users/mccown/test1/file1_2.txt";
        fs::copy(file_name, file_name_copy_1).unwrap();
        let _enc_file_name_copy_1 = encrypt_file_i16(
            k.get_did(), 
            k.get_private_key(), 
            k.get_did(), 
            k.get_public_key(),
            file_name_copy_1.to_string(), 
            file_data.clone(),
            source_root.clone(),
            dest_root.clone()
        );

        fs::copy(file_name, file_name_copy_2).unwrap();
        let enc_file_name_copy_2 = encrypt_file_i16(
            k.get_did(), 
            k.get_private_key(), 
            k.get_did(), 
            k.get_public_key(),
            file_name_copy_2.to_string(), 
            file_data,
            source_root.clone(),
            dest_root.clone()
        );

        println!("\n\nSleeping before deletes...");
        thread::sleep(time::Duration::from_millis(5000));
        println!("Now doing deletes.");

        delete_plaintext_file(
            k.get_did(),
            file_name_copy_1.to_string(),
            source_root.clone(),
            dest_root.clone()
        );

        delete_encrypted_file(
            enc_file_name_copy_2.to_string(),
            source_root.clone(),
            dest_root.clone()
        );

        // Return void.
        ()
    }
}
