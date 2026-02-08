import base64

# Read the base64 string from the file
with open("base64_input.txt", "r") as f:
    base64_string = f.read().strip()

# Decode the base64 string
decoded_bytes = base64.b64decode(base64_string)

# Write the decoded bytes to a file
with open("decoded_file.gcode", "wb") as f:
    f.write(decoded_bytes)
