#ifndef STRINGSTORAGE_H
#define STRINGSTORAGE_H

#include <string>
#include <map>
#include <set>
#include <limits>

namespace annis
{
const std::uint32_t STRING_STORAGE_ANY = 0;

class StringStorage
{
public:
  StringStorage();

  const std::string& str(std::uint32_t id) const
  {
    typedef std::map<std::uint32_t, std::string>::const_iterator ItType;
    ItType it = stringStorageByID.find(id);
    if(it != stringStorageByID.end())
    {
      return it->second;
    }
    else
    {
      throw("Unknown string ID");
    }
  }

  std::pair<bool, std::uint32_t> findID(const std::string& str) const
  {
    typedef std::map<std::string, std::uint32_t>::const_iterator ItType;
    std::pair<bool, std::uint32_t> result;
    result.first = false;
    result.second = 0;
    ItType it = stringStorageByValue.find(str);
    if(it != stringStorageByValue.end())
    {
      result.first = true;
      result.second = it->second;
    }
    return result;
  }

  uint32_t lower_bound(const std::string& str) const
  {
    auto it = stringStorageByValue.lower_bound(str);
    if(it != stringStorageByValue.end())
    {
      return it->second;
    }
    return std::numeric_limits<uint32_t>::min();
  }

  uint32_t upper_bound(const std::string& str) const
  {
    auto it = stringStorageByValue.upper_bound(str);
    if(it != stringStorageByValue.end())
    {
      return it->second;
    }
    return std::numeric_limits<uint32_t>::max();
  }

  std::set<std::uint32_t> findRegex(const std::string& str) const;

  std::uint32_t add(const std::string& str);

  void clear();
  bool load(const std::string& dirPath);
  bool save(const std::string &dirPath);
  size_t size() {return stringStorageByID.size();}


private:
  std::map<std::uint32_t, std::string> stringStorageByID;
  std::map<std::string, std::uint32_t> stringStorageByValue;

};
}

#endif // STRINGSTORAGE_H
