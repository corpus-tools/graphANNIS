#ifndef EXACTANNOKEYSEARCH_H
#define EXACTANNOKEYSEARCH_H

#include "annotationsearch.h"
#include <stx/btree_map>

namespace annis
{

class ExactAnnoKeySearch : public AnnotationKeySearch
{
  using ItType = stx::btree_multimap<Annotation, nodeid_t>::const_iterator;

public:
  /**
   * @brief Find all annotations.
   * @param db
   */
  ExactAnnoKeySearch(const DB& db);
  /**
   * @brief Find annotations by name
   * @param db
   * @param annoName
   */
  ExactAnnoKeySearch(const DB& db, const std::string& annoName);
  ExactAnnoKeySearch(const DB& db, const std::string& annoNamspace, const std::string& annoName);

  virtual ~ExactAnnoKeySearch();

  virtual bool hasNext()
  {
    return it != db.inverseNodeAnnotations.end() && it != itEnd;
  }

  virtual Match next();
  virtual void reset();

  const std::set<AnnotationKey>& getValidAnnotationKeys()
  {
    if(!validAnnotationKeysInitialized)
    {
      initializeValidAnnotationKeys();
    }
    return validAnnotationKeys;
  }

private:
  const DB& db;

  ItType it;
  ItType itBegin;
  ItType itEnd;

  bool validAnnotationKeysInitialized;
  std::set<AnnotationKey> validAnnotationKeys;

  bool currentMatchValid;
  Match currentMatch;

  void initializeValidAnnotationKeys();

};


} // end namespace annis
#endif // EXACTANNOKEYSEARCH_H
