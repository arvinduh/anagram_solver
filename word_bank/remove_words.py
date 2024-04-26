"""
This module contains the functions to remove words from the word bank.
"""
import os


def remove_words(
    fn: str,
    dst_dir: str = './files/sorted/',
) -> bool:
    """
    Remove words from the word bank.
    :param fn: The file containing the words to remove.
    :param dst_dir: The directory containing the word bank files.
    :return: True if the words were removed successfully, False otherwise.
    """
    if not os.path.exists(fn):
        return False
    
    with open(fn, 'r') as file:
        rem_words = file.readlines()
    
    rem_words = [word.strip().lower() for word in rem_words]
    
    for i in range(3, 8):
        src_fn = os.path.join(dst_dir, f'{i}_letter_words.txt')
        with open(src_fn, 'r') as src_file:
            src_words = src_file.readlines()
        
        src_words = [word.strip().lower() for word in src_words]
        src_words = [word for word in src_words if word not in rem_words]
        src_words = set(src_words)

        with open(src_fn, 'w') as src_file:
            for word in src_words:
                src_file.write(word + '\n')
    
    return True


if __name__ == '__main__':
    pass