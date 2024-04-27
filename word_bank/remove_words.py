"""
This module contains the functions to remove words from the word bank.
"""
import os
import config
from .file_connection import find_file


def remove_words(
    fn: str,
    dst_dir: str = config.SORTED_FILES,
) -> bool:
    """
    Remove words from the word bank.
    :param fn: The file containing the words to remove.
    :param dst_dir: The directory containing the word bank files.
    :return: True if the words were removed successfully, False otherwise.
    """
    # Find if file is valid
    fn = find_file(fn)

    # If not, return False
    if fn is None:
        return False
    
    # if dst_dir doesn't exist, return False
    if not os.path.exists(dst_dir):
        return False

    # Read the words to remove
    with open(fn, 'r') as file:
        rem_words = file.readlines()
        rem_words = [word.strip().lower() for word in rem_words]
    
    # For each word length, remove words to remove of that length
    for i in range(config.MIN_LENGTH, config.MAX_LENGTH + 1):
        src_fn = os.path.join(dst_dir, config.FN_FORMAT.format(letter_count=i))

        # If no file of length i, skip
        if not os.path.exists(src_fn):
            continue
        
        # Open file
        with open(src_fn, 'r') as src_file:
            src_words = src_file.readlines()
        
        # Filter for words not in rem_words
        src_words = [word.strip().lower() for word in src_words]
        src_words = [word for word in src_words if word not in rem_words]
        src_words = set(src_words)

        # Write remaining words
        with open(src_fn, 'w') as src_file:
            for word in src_words:
                src_file.write(word + '\n')
    
    return True


if __name__ == '__main__':
    pass