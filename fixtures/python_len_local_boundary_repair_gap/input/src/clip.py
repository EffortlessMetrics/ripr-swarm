def truncate_visible(text, limit):
    visible = len(text)
    if visible > limit:
        return text[:limit]
    return text
