#include "annotationsearch.h"

using namespace annis;
using namespace std;

AnnotationNameSearch::AnnotationNameSearch(DB& db, const string& annoName)
  : db(db)
{
  std::pair<bool, uint32_t> searchResult = db.strings.findID(annoName);

  if(searchResult.first)
  {
    Annotation lowerKey;
    lowerKey.name = searchResult.second;
    lowerKey.ns = numeric_limits<uint32_t>::min();
    lowerKey.val = numeric_limits<uint32_t>::min();

    Annotation upperKey;
    upperKey.name = searchResult.second;
    upperKey.ns = numeric_limits<uint32_t>::max();
    upperKey.val = numeric_limits<uint32_t>::max();

    itBegin = db.inverseNodeAnnotations.lower_bound(lowerKey);
    it = itBegin;
    itEnd = db.inverseNodeAnnotations.upper_bound(upperKey);
  }
  else
  {
    itBegin = db.inverseNodeAnnotations.end();
    it = itBegin;
    itEnd = db.inverseNodeAnnotations.end();
  }
}

AnnotationNameSearch::AnnotationNameSearch(DB &db, const string &annoNamspace, const string &annoName)
  : db(db)
{
  std::pair<bool, uint32_t> nameID = db.strings.findID(annoName);
  std::pair<bool, uint32_t> namspaceID = db.strings.findID(annoNamspace);

  if(nameID.first && namspaceID.first)
  {
    Annotation lowerKey;
    lowerKey.name = nameID.second;
    lowerKey.ns = namspaceID.second;
    lowerKey.val = numeric_limits<uint32_t>::min();

    Annotation upperKey;
    upperKey.name = nameID.second;
    upperKey.ns = namspaceID.second;
    upperKey.val = numeric_limits<uint32_t>::max();

    itBegin = db.inverseNodeAnnotations.lower_bound(lowerKey);
    it = itBegin;
    itEnd = db.inverseNodeAnnotations.upper_bound(upperKey);
  }
  else
  {
    itBegin = db.inverseNodeAnnotations.end();
    it = itBegin;
    itEnd = db.inverseNodeAnnotations.end();
  }
}

AnnotationNameSearch::AnnotationNameSearch(DB &db, const string &annoNamspace, const string &annoName, const string &annoValue)
  :db(db)
{
  std::pair<bool, uint32_t> nameID = db.strings.findID(annoName);
  std::pair<bool, uint32_t> namspaceID = db.strings.findID(annoNamspace);
  std::pair<bool, uint32_t> valueID = db.strings.findID(annoValue);

  if(nameID.first && namspaceID.first && valueID.first)
  {
    Annotation key;
    key.name = nameID.second;
    key.ns = namspaceID.second;
    key.val = valueID.second;

    itBegin = db.inverseNodeAnnotations.lower_bound(key);
    it = itBegin;
    itEnd = db.inverseNodeAnnotations.upper_bound(key);
  }
  else
  {
    itBegin = db.inverseNodeAnnotations.end();
    it = itBegin;
    itEnd = db.inverseNodeAnnotations.end();
  }
}



Match AnnotationNameSearch::next()
{
  Match result;
  if(hasNext())
  {
    result.first = it->second; // node ID
    result.second = it->first; // annotation itself
    it++;
  }
  return result;
}

void AnnotationNameSearch::reset()
{
  it = itBegin;
}

AnnotationNameSearch::~AnnotationNameSearch()
{

}
