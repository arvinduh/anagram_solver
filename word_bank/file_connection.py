import os
import shutil
import config


def del_dirs(
    path: str,
    recursive: bool = True
) -> bool:
    # Is file a directory
    if not os.path.isdir(path):
        return False
    
    # If directory doesn't exist
    try:
        dirs = len(os.listdir(path))
    except FileNotFoundError:
        return False
    
    # If directory is not empty
    if dirs:
        return False
    
    # If directory is empty, delete it
    shutil.rmtree(path)
    if recursive:
        del_dirs(os.path.dirname(path, True))
    return True


def find_file(
    fn: str
) -> str | None:
    
    src_dir = os.path.abspath(config.SOURCE_FILES)

    # Check if file exists
    if os.path.exists(fn):
        # If file is in src_dir, return it
        if os.path.abspath(fn).startswith(src_dir + os.sep):
            return os.path.relpath(fn, os.getcwd())
        
        # If file isn't in src_dir, move it to src_dir
        shutil.move(fn, os.path.join(src_dir, os.path.basename(fn)))
        del_dirs(os.path.dirname(fn), True)
        return os.path.join(src_dir, os.path.basename(fn))
    
    # check if file exists in source directory
    elif os.path.exists(os.path.join(src_dir, fn)):
        return os.path.relpath(os.path.join(src_dir, fn), os.getcwd())
    
    # If file does not exist in either location
    else:
        return None