// Import Java resources.
import java.util.concurrent.Semaphore;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.FileInputStream;
import java.io.FileOutputStream;

import java.nio.file.Files;
import java.nio.charset.Charset;
import java.nio.file.DirectoryStream;
import java.nio.file.FileSystems;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.nio.file.Paths;
import static java.nio.file.StandardCopyOption.*;
import java.nio.file.StandardWatchEventKinds;
import java.nio.file.WatchEvent;
import java.nio.file.WatchKey;
import java.nio.file.WatchService;
import java.security.KeyPair;
import java.util.Arrays;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;
import java.lang.Byte;

import com.sun.nio.file.SensitivityWatchEventModifier;

// Import uniffi interface for didcomm_communications.
import uniffi.didcomm_communications.*;

//-----------------------------------------------------------------------------
public class UserEncryptionSync extends Thread {

    final int MAX_FILE_SIZE = 32768;
    final boolean destEncrypted;

    private final WatchService watcher;
    // private static Map<WatchKey, Path> keyMap = new HashMap<>();
    // private static int data = 20;

    private final Path sourceDirectory;
    private final Path destinationDirectory;

    private final uniffi.didcomm_communications.KeyPair keyPair;

    private final Semaphore semaphore;

    //-------------------------------------------------------------------------
    UserEncryptionSync(Semaphore semaphore, Path sourceDirectory, Path destinationDirectory, boolean destEncrypted) throws IOException {

        this.semaphore = semaphore;

        System.out.format( "Synchronizing files from %s to %s...\n", sourceDirectory.toString(), destinationDirectory.toString());

        // Designates whether the destination is encrypted or decrypted.
        this.destEncrypted = destEncrypted;

        // Create the watch service.
        this.watcher = FileSystems.getDefault().newWatchService();

        // Register the directory with the watcher service.
        registerDirectory(sourceDirectory, watcher);

        // Save the source and destination directories.
        this.sourceDirectory = sourceDirectory;
        this.destinationDirectory = destinationDirectory;

        // Create the encryption keypair.
        // For this tutorial, the keypair generated is always the same.
        // In practice, the keypairs should be random and managed accordingly.
        keyPair = Didcomm_communicationsKt.generateKeyPair("6QN8DfuN9hjgHgPvLXqgzqYE3jRRGRrmJQZkd5tL8paR");
        System.out.println("\n\n ------ from generate_key_pair() ------ \n");
        System.out.println("did = " + keyPair.getDid());
        System.out.println("pubkey = " + keyPair.getPublicKey());
        System.out.println("privkey = " + keyPair.getPrivateKey());
        System.out.println("\n\n--------------------------------------\n");
    }

    //-------------------------------------------------------------------------
    private void registerDirectory(Path sourceDirectory, WatchService watcher) throws IOException {

        // Only register directories and don't follow symbolic links.
        if (!Files.isDirectory(sourceDirectory, LinkOption.NOFOLLOW_LINKS)) {
            return;
        }

        // Register the file system watcher.
        // The sensitivity value makes it detect changes quickly.
        WatchKey key = sourceDirectory.register(watcher,
                new WatchEvent.Kind[]{    
                    StandardWatchEventKinds.ENTRY_CREATE,
                    StandardWatchEventKinds.ENTRY_DELETE,
                    StandardWatchEventKinds.ENTRY_MODIFY},
            SensitivityWatchEventModifier.HIGH);

        // Save the WatchKey
        // keyMap.put(key, sourceDirectory);

        //---------------------------------------------
        //  ***** For simplicity, ignore subdirectories *****
        // // Now, look for subdirectories.
        // for (File f : sourceDirectory.toFile().listFiles()) {
        //     if (Files.isDirectory(sourceDirectory, LinkOption.NOFOLLOW_LINKS)) { 
        //         registerDirectory(f.toPath(), watcher);
        //     }
        // }
    }

    //-------------------------------------------------------------------------
    private boolean simpleCopyFileEncrypt(String inputFile) throws IOException, InterruptedException {

        boolean copied = false;

        semaphore.acquire();

        // Test to check if inputFile is newer than outputFile.
        File inFile = new File(inputFile);

        InputStream fis = null;
        try {
            fis = new FileInputStream(inFile);
            if (fis != null) {
                byte[] plaintext = new byte[MAX_FILE_SIZE];
                int length;
                if ((length = fis.read(plaintext)) > 0) {

                    // Convert to the expected List<Byte>
                    List<Byte> file_data = new ArrayList<Byte>();
                    for (int i = 0; i < length; i++) {
                        file_data.add(Byte.valueOf(plaintext[i]));
                    }

                    // Encrypt the plaintext to a new file in the dest directory.
                    String enc_file_name = Didcomm_communicationsKt.encryptFileI8(
                        keyPair.getDid(), 
                        keyPair.getPrivateKey(), 
                        keyPair.getDid(), 
                        keyPair.getPublicKey(), 
                        inputFile,
                        file_data,
                        this.sourceDirectory.toString(),
                        this.destinationDirectory.toString());
                }

                copied = true;  
            }
        } catch (IOException e) {
                
        } finally {

            if (fis != null) {
                fis.close();
            }
        }
    
        semaphore.release();

        return copied;
    }

    //-------------------------------------------------------------------------
    private boolean simpleCopyFileDecrypt(String inputFile) throws IOException, InterruptedException {

        boolean copied = false;

        semaphore.acquire();
        
        String enc_data = Didcomm_communicationsKt.decryptFileMessage(
            inputFile,
            keyPair.getPrivateKey(),
            keyPair.getPublicKey(),
            this.destinationDirectory.toString()
        );

        if (!enc_data.equals("")) {
            copied = true; 
        } 

        semaphore.release();

        return copied;
    }

    //-------------------------------------------------------------------------
    private boolean copyFile(String fullPathString) throws IOException {

        boolean rValue = false;

        try {
            if (this.destEncrypted) {

                rValue = simpleCopyFileEncrypt(fullPathString);
            } else {

                rValue = simpleCopyFileDecrypt(fullPathString);
            }
        } catch (InterruptedException ie) {

        }

        return rValue;
    }

    //-------------------------------------------------------------------------
    private boolean deleteFile(String fullPathString) {

        boolean rValue = false;

        if (this.destEncrypted) {
            Didcomm_communicationsKt.deletePlaintextFile(
                keyPair.getDid(),
                fullPathString,
                this.sourceDirectory.toString(),
                this.destinationDirectory.toString());
        } else {

            Didcomm_communicationsKt.deleteEncryptedFile(
                fullPathString,
                this.sourceDirectory.toString(),
                this.destinationDirectory.toString());
        }

        return rValue;
    }

    //-------------------------------------------------------------------------
    public void run() {

        // Have this thread run as an endless loop.
        for (;;) {
            
            // Wait for a watcher event to be detected.
            WatchKey key;
            try {
                key = watcher.take();
            } catch (InterruptedException x) {
                return;
            }

            // Process each event that was received.
            for (WatchEvent<?> event: key.pollEvents()) {
                WatchEvent.Kind<?> kind = event.kind();

                // For this tutorial, please skip errors.
                if (kind == StandardWatchEventKinds.OVERFLOW) {
                    continue;
                }

                // Get the name of the file corresponding to the event.
                @SuppressWarnings("unchecked")
                WatchEvent<Path> ev = (WatchEvent<Path>)event;
                Path context = ev.context();
                String filename = context.toString();
                String kindString = "";

                // Adding a check to skip those pesky MacOS .DS_Store files.
                if (!filename.equals(".DS_Store")) {

                    // Check the event for specific types.
                    if (kind == StandardWatchEventKinds.ENTRY_CREATE) {
                        if (Files.isDirectory(context)) {
                            // ***** For now, ignore subdirectories *****
                            break;
                        }
                        
                        // Get the file name and path information.
                        Path dir = (Path)key.watchable();
                        Path fullPath = dir.resolve(context);
                        String fullPathString = fullPath.toString();

                        // Copy the file.
                        try {
                            if (this.copyFile(fullPathString)) {
                                kindString = "created";
                            }
                        } catch (IOException e) {
                            e.printStackTrace();
                        }
                    }
                    else if (kind == StandardWatchEventKinds.ENTRY_DELETE) {
                        // For a delete event, delete the corresponding file.
                        if (Files.isDirectory(context)) {
                            // ***** For now, ignore subdirectories *****
                            break;
                        }

                        // Get the file name and path information.
                        Path dir = (Path)key.watchable();
                        Path fullPath = dir.resolve(context);
                        String fullPathString = fullPath.toString();

                        deleteFile(fullPathString);
                        kindString = "deleted";
                    }
                    else if (kind == StandardWatchEventKinds.ENTRY_MODIFY) {
                        // If a file was modified, copy it as well.
                        if (Files.isDirectory(context)) {
                            // ***** For now, ignore subdirectories *****
                            break;
                        }
                        
                        // Get the file name and path information.
                        Path dir = (Path)key.watchable();
                        Path fullPath = dir.resolve(context);
                        String fullPathString = fullPath.toString();

                        // Copy the file.
                        try {
                            if (this.copyFile(fullPathString)) {
                                kindString = "modified";
                            }
                        } catch (IOException e) {
                            e.printStackTrace();
                        }
                    }
                    else if (kind == StandardWatchEventKinds.OVERFLOW) {
                        kindString = kind.toString();
                        filename = "had an Overflow error";

                        // For this tutorial, please skip errors.
                    }

                    // For monitoring and debugging, print the filename that was changed.
                    if (!kindString.equals("")) {
                        System.out.format("The file, %s, was %s.\n", filename, kindString);
                    }
                }
            }

            // Reset the key to keep watching for new events.
            if (!key.reset()) {
                // The key became invalid.
                // keyMap.remove(key);
            }
            // if(keyMap.isEmpty()){
            //     break;
            // }
        }
    }

    //-------------------------------------------------------------------------
    protected static void tests() {

        System.out.println("\n\n---- from generate_key_pair() ----\n");
        uniffi.didcomm_communications.KeyPair k = Didcomm_communicationsKt.generateKeyPair("6QN8DfuN9hjgHgPvLXqgzqYE3jRRGRrmJQZkd5tL8paR");
        System.out.println("\n\n ------ from generate_key_pair() ------ \n");
        System.out.println("did = " + k.getDid());
        System.out.println("pubkey = " + k.getPublicKey());
        System.out.println("privkey = " + k.getPrivateKey());
        System.out.println("\n\n--------------------------------------\n");

        // Simulate reading data from a file, because the file may not exist.
        System.out.println("\n\n---- test data ----\n");
        String filename = "/Users/mccown/test1/file1.txt";
        Short[] fileData = {0x41, 0x41, 0x41, 0xEA};
        java.util.List<Short> fileDataArray = java.util.Arrays.asList(fileData);
        System.out.println("     File name = " + filename);
        System.out.println("     Plaintext input data = " + fileDataArray);

        System.out.println("\n\n---- from encrypt_message() ----\n");
        String sourceRoot = "/Users/mccown/test1";
        String destRoot = "/Users/mccown/test2";
        String encFilePath = Didcomm_communicationsKt.encryptFileI16(
            k.getDid(), 
            k.getPrivateKey(), 
            k.getDid(), 
            k.getPublicKey(),
            filename, 
            fileDataArray,
            sourceRoot,
            destRoot
        );
        System.out.println("     Encrypted file path = " + encFilePath);

        System.out.println("\n\n---- from decrypt_file_message() ----\n");
        String output_file = Didcomm_communicationsKt.decryptFileMessage(
            encFilePath,
            k.getPrivateKey(),
            k.getPublicKey(),
            sourceRoot
        );

        System.out.println("      Decrypted output file = " + output_file);

        // ---- Deletes ----
        try {
            Path filename_copy_1 = Paths.get("/Users/mccown/test1/file1_1.txt");
            Path filename_copy_2 = Paths.get("/Users/mccown/test1/file1_2.txt");
            Files.copy(Paths.get(filename), filename_copy_1, REPLACE_EXISTING);
            String enc_file_name_copy_1 = Didcomm_communicationsKt.encryptFileI16(
                k.getDid(), 
                k.getPrivateKey(), 
                k.getDid(), 
                k.getPublicKey(),
                filename_copy_1.toString(), 
                fileDataArray,
                sourceRoot,
                destRoot
            );

            Files.copy(Paths.get(filename), filename_copy_2, REPLACE_EXISTING);
            String enc_file_name_copy_2 = Didcomm_communicationsKt.encryptFileI16(
                k.getDid(), 
                k.getPrivateKey(), 
                k.getDid(), 
                k.getPublicKey(),
                filename_copy_2.toString(), 
                fileDataArray,
                sourceRoot,
                destRoot
            );

            System.out.println("\n\nSleeping before deletes...");
            try {
                Thread.sleep(5000);
            } catch (InterruptedException ie) {

            }
            System.out.println("Now doing deletes.");

            Didcomm_communicationsKt.deletePlaintextFile(
                k.getDid(),
                filename_copy_1.toString(),
                sourceRoot.toString(),
                destRoot.toString()
            );

            Didcomm_communicationsKt.deleteEncryptedFile(
                enc_file_name_copy_2,
                sourceRoot,
                destRoot
            );
        } catch (IOException ioe) {
        }
    }

    //-------------------------------------------------------------------------
    public static void main(String[] args)
    {
        // UserEncryptionSync.tests();

        if (args.length != 2) {
            System.out.println("\nUsage:  java UserEncryptionSync source dest\n");
            return;
        }

        // Build the absolute paths from those relative (to home) paths
        // specified on the commandline.
        String home = System.getProperty("user.home");
        Path sourcePath = java.nio.file.Paths.get(home, args[0]);
        Path destPath = java.nio.file.Paths.get(home, args[1]);

        // Setup semaphore.
        Semaphore semaphore = new Semaphore(1);

        UserEncryptionSync ues1;
        UserEncryptionSync ues2;
        try {
            // If it doesn't exist, make the source directory.
            if (!Files.exists(sourcePath)){
                java.nio.file.Files.createDirectory(sourcePath);
            }

            // If it doesn't exist, make the dest directory.
            if (!Files.exists(destPath)){
                java.nio.file.Files.createDirectory(destPath);
            }

            // Start the ues1 thread.
            System.out.println("Starting source -> dest...");
            ues1 = new UserEncryptionSync(semaphore, sourcePath, destPath, true);
            ues1.start();

            // Start the ues2 thread.
            System.out.println("Starting dest -> source...");
            ues2 = new UserEncryptionSync(semaphore, destPath, sourcePath, false);
            ues2.start();
        } catch (IOException e) {
            e.printStackTrace();
        }
    }
}